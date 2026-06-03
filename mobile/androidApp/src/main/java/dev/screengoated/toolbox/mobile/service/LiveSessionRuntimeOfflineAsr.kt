package dev.screengoated.toolbox.mobile.service

import android.content.Context
import android.os.SystemClock
import android.util.Log
import dev.screengoated.toolbox.mobile.AppToastBus
import dev.screengoated.toolbox.mobile.capture.AudioCaptureController
import dev.screengoated.toolbox.mobile.model.AndroidLiveSessionRepository
import dev.screengoated.toolbox.mobile.model.RealtimeModelIds
import dev.screengoated.toolbox.mobile.service.tts.RealtimeTtsCoordinator
import dev.screengoated.toolbox.mobile.service.tts.TtsRuntimeService
import dev.screengoated.toolbox.mobile.shared.live.DisplayMode
import dev.screengoated.toolbox.mobile.shared.live.LiveTranslationModelCatalog
import dev.screengoated.toolbox.mobile.shared.live.SessionPhase
import dev.screengoated.toolbox.mobile.shared.live.SourceMode
import dev.screengoated.toolbox.mobile.shared.live.TranscriptionMethod
import dev.screengoated.toolbox.mobile.storage.ProjectionConsentStore
import dev.screengoated.toolbox.mobile.ui.i18n.apiKeyErrorToastText
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext


// Offline ASR (Moonshine + Sherpa/Zipformer) session runners extracted from LiveSessionRuntime.
internal suspend fun LiveSessionRuntime.runMoonshineSession() {
    val modelId = repository.currentConfig().transcriptionProvider.id
    val langCode = repository.currentConfig().transcriptionLanguage

    // Zipformer models route to sherpa-onnx
    if (modelId == "zipformer") {
        Log.i("Sherpa", "Zipformer requested, langCode='$langCode', modelId='$modelId'")
        val zipLang = dev.screengoated.toolbox.mobile.service.moonshine.ZipformerLanguage.fromCode(langCode)
            ?: dev.screengoated.toolbox.mobile.service.moonshine.ZipformerLanguage.ENGLISH
        Log.i("Sherpa", "Resolved to: ${zipLang.name} (${zipLang.modelName})")
        val moonshineManager = dev.screengoated.toolbox.mobile.service.moonshine.MoonshineModelManager(context)
        if (!moonshineManager.isZipformerInstalled(zipLang)) {
            downloadCancelAction = { sessionJob?.cancel() }
            overlayController.showDownloadModal("Zipformer ${zipLang.displayName}")
            try {
                withContext(Dispatchers.IO) {
                    val progressJob = launch(Dispatchers.Main) {
                        moonshineManager.downloadState.collect { state ->
                            when (state) {
                                is dev.screengoated.toolbox.mobile.service.moonshine.MoonshineModelManager.DownloadState.Downloading -> {
                                    overlayController.updateDownloadProgress(state.progress * 100, state.currentFile)
                                }
                                else -> {}
                            }
                        }
                    }
                    try { moonshineManager.downloadZipformer(zipLang) } finally { progressJob.cancel() }
                }
                overlayController.hideDownloadModal()
                downloadCancelAction = null
            } catch (e: CancellationException) {
                overlayController.hideDownloadModal()
                downloadCancelAction = null
                throw e
            }
            // Guard: download may have failed internally (DownloadState.Error) without throwing
            if (!moonshineManager.isZipformerInstalled(zipLang)) {
                val state = moonshineManager.downloadState.value
                val msg = if (state is dev.screengoated.toolbox.mobile.service.moonshine.MoonshineModelManager.DownloadState.Error)
                    state.message else "Download failed for ${zipLang.displayName}"
                error(msg)
            }
        }
        runSherpaSession(zipLang, moonshineManager)
        return
    }

    // Moonshine models (English-only, pick variant by model ID)
    val lang = dev.screengoated.toolbox.mobile.service.moonshine.MoonshineLanguage.forModelId(modelId)
    val moonshineManager = dev.screengoated.toolbox.mobile.service.moonshine.MoonshineModelManager(context)

    // Download model if needed
    if (!moonshineManager.isInstalled(lang)) {
        downloadCancelAction = { sessionJob?.cancel() }
        overlayController.showDownloadModal("Moonshine ${lang.displayName}")
        try {
            withContext(Dispatchers.IO) {
                val progressJob = launch(Dispatchers.Main) {
                    moonshineManager.downloadState.collect { state ->
                        when (state) {
                            is dev.screengoated.toolbox.mobile.service.moonshine.MoonshineModelManager.DownloadState.Downloading -> {
                                overlayController.updateDownloadProgress(state.progress * 100, state.currentFile)
                            }
                            else -> {}
                        }
                    }
                }
                try {
                    moonshineManager.download(lang)
                } finally {
                    progressJob.cancel()
                }
            }
            overlayController.hideDownloadModal()
            downloadCancelAction = null
        } catch (e: CancellationException) {
            overlayController.hideDownloadModal()
            downloadCancelAction = null
            throw e
        }
        if (!moonshineManager.isInstalled(lang)) {
            val state = moonshineManager.downloadState.value
            val message = if (state is dev.screengoated.toolbox.mobile.service.moonshine.MoonshineModelManager.DownloadState.Error) {
                state.message
            } else {
                "Download failed for ${lang.displayName}"
            }
            error(message)
        }
    }

    withContext(Dispatchers.IO) {
        // Create Moonshine Transcriber and load from filesystem
        val transcriber = ai.moonshine.voice.Transcriber()
        val modelPath = moonshineManager.modelDir(lang).absolutePath
        transcriber.loadFromFiles(modelPath, lang.moonshineArch)
        Log.i("Moonshine", "Loaded ${lang.modelName} from $modelPath (arch=${lang.moonshineArch})")

        // Windows-style committed + draft transcript management.
        // committed_history = all finalized lines joined
        // draft = current partial line text (may be revised by Moonshine)
        // publish_transcript(committed, draft) replaces the full transcript
        // Windows-style committed + draft transcript management.
        // - committedHistory: finalized text (periods are real)
        // - currentDraft: partial text (trailing periods STRIPPED, matching
        //   Windows streaming.rs — prevents premature translation commits)
        // - Stale draft: add period after 3s no change to force translation
        var committedHistory = ""
        var currentDraft = ""
        var lastDraftChangeMs = SystemClock.elapsedRealtime()
        val DRAFT_STALE_MS = 3_000L

        fun stripTrailingSentenceMarks(text: String): String {
            val trimmed = text.trimEnd()
            return if (trimmed.endsWith('.') || trimmed.endsWith('?') || trimmed.endsWith('!')) {
                trimmed.trimEnd('.', '?', '!')
            } else {
                text
            }
        }

        fun publishTranscript(committed: String, draft: String) {
            // If draft hasn't changed for 3s, add period to force translation
            val draftToPublish = if (draft.isNotBlank()
                && SystemClock.elapsedRealtime() - lastDraftChangeMs >= DRAFT_STALE_MS
            ) {
                "${draft.trimEnd()}."
            } else {
                draft
            }
            repository.setTranscriptSegments(
                committed = committed,
                draft = draftToPublish,
                nowMs = SystemClock.elapsedRealtime(),
            )
        }

        transcriber.addListener { event ->
            event.accept(object : ai.moonshine.voice.TranscriptEventListener() {
                override fun onLineTextChanged(e: ai.moonshine.voice.TranscriptEvent.LineTextChanged) {
                    val text = e.line.text ?: return
                    // Strip trailing sentence marks from draft (matching Windows)
                    // Real periods survive into committedHistory when onLineCompleted fires
                    val stripped = stripTrailingSentenceMarks(text)
                    if (stripped != currentDraft) {
                        currentDraft = stripped
                        lastDraftChangeMs = SystemClock.elapsedRealtime()
                    }
                    publishTranscript(committedHistory, currentDraft)
                }

                override fun onLineCompleted(e: ai.moonshine.voice.TranscriptEvent.LineCompleted) {
                    val text = e.line.text ?: return
                    if (text.isNotBlank()) {
                        // Completed line keeps its punctuation (confirmed real)
                        committedHistory = if (committedHistory.isEmpty()) {
                            text
                        } else {
                            "$committedHistory $text"
                        }
                        currentDraft = ""
                        lastDraftChangeMs = SystemClock.elapsedRealtime()
                        publishTranscript(committedHistory, "")
                    }
                }
            })
        }

        try {
            repository.markListening()

            transcriber.start()

            val audioFlow = audioCaptureController.open(
                config = repository.currentConfig(),
                onRms = { rms -> overlayController.updateVolume(rms) },
            )

            // Batch audio into 500ms chunks before feeding Moonshine.
            // Tiny per-call JNI overhead × 25 calls/sec was the bottleneck.
            val BATCH_SAMPLES = 16000 / 2 // 500ms at 16kHz = 8000 samples
            val audioBatch = mutableListOf<Float>()
            val batchLock = Any()

            val collectorJob = launch {
                audioFlow.collect { chunk ->
                    synchronized(batchLock) {
                        for (s in chunk) audioBatch.add(s / 32768f)
                    }
                }
            }

            var totalSamplesFed = 0L
            val startTimeMs = SystemClock.elapsedRealtime()

            try {
                while (collectorJob.isActive && currentCoroutineContext().isActive) {
                    val batch: FloatArray?
                    synchronized(batchLock) {
                        if (audioBatch.size >= BATCH_SAMPLES) {
                            // If buffer > 10 seconds, drop old audio to catch up.
                            // Moonshine processes large batches efficiently, so we
                            // take everything — but don't let it grow unbounded.
                            val maxBuffer = 16000 * 10 // 10 seconds
                            if (audioBatch.size > maxBuffer) {
                                val drop = audioBatch.size - maxBuffer
                                audioBatch.subList(0, drop).clear()
                                Log.w("Moonshine", "Dropped ${drop / 16000}s of old audio to catch up")
                            }
                            batch = FloatArray(audioBatch.size) { audioBatch[it] }
                            audioBatch.clear()
                        } else {
                            batch = null
                        }
                    }

                    if (batch != null) {
                        transcriber.addAudio(batch, 16000)
                        totalSamplesFed += batch.size

                        val audioTimeMs = totalSamplesFed * 1000 / 16000
                        val wallTimeMs = SystemClock.elapsedRealtime() - startTimeMs
                        val lagMs = wallTimeMs - audioTimeMs
                        Log.i("Moonshine", "fed=${audioTimeMs/1000}s wall=${wallTimeMs/1000}s lag=${lagMs}ms batch=${batch.size}")
                    } else {
                        delay(50)
                    }
                }
            } finally {
                collectorJob.cancel()
            }
        } finally {
            transcriber.stop()
        }
    }
}

internal suspend fun LiveSessionRuntime.runSherpaSession(
    lang: dev.screengoated.toolbox.mobile.service.moonshine.ZipformerLanguage,
    modelManager: dev.screengoated.toolbox.mobile.service.moonshine.MoonshineModelManager,
) {
    withContext(Dispatchers.IO) {
        // Free JVM heap before native model allocation
        System.gc()
        val actMgr = context.getSystemService(android.app.ActivityManager::class.java)
        val memInfo = android.app.ActivityManager.MemoryInfo()
        actMgr.getMemoryInfo(memInfo)
        val availMb = memInfo.availMem / 1_048_576L
        Log.i("Sherpa", "Available RAM before load: ${availMb}MB (lowMemory=${memInfo.lowMemory})")
        if (memInfo.lowMemory || availMb < 150L) {
            error("Not enough RAM to load ${lang.displayName} model (${availMb}MB available, need ~150MB free)")
        }

        val modelDir = modelManager.zipformerDir(lang).absolutePath
        val bpeVocabPath = lang.bpeVocabFile?.let { "$modelDir/$it" } ?: ""
        Log.i("Sherpa", "Loading ${lang.displayName}: encoder=${lang.sherpaEncoder()} bpe=${bpeVocabPath.ifEmpty { "none" }}")

        val config = com.k2fsa.sherpa.onnx.OnlineRecognizerConfig(
            modelConfig = com.k2fsa.sherpa.onnx.OnlineModelConfig(
                transducer = com.k2fsa.sherpa.onnx.OnlineTransducerModelConfig(
                    encoder = "$modelDir/${lang.sherpaEncoder()}",
                    decoder = "$modelDir/${lang.sherpaDecoder()}",
                    joiner = "$modelDir/${lang.sherpaJoiner()}",
                ),
                tokens = "$modelDir/tokens.txt",
                modelType = lang.sherpaModelType,
                numThreads = 1,
                bpeVocab = bpeVocabPath,
            ),
            enableEndpoint = true,
            decodingMethod = "greedy_search",
        )

        val recognizer = com.k2fsa.sherpa.onnx.OnlineRecognizer(config = config)
        try {
            val ptrField = recognizer.javaClass.getDeclaredField("ptr")
            ptrField.isAccessible = true
            if (ptrField.getLong(recognizer) == 0L) {
                error("${lang.displayName} model failed to load — check model files are complete")
            }
        } catch (e: IllegalStateException) {
            throw e  // re-throw our own error()
        } catch (_: Exception) {}
        val stream = recognizer.createStream()
        Log.i("Sherpa", "Loaded ${lang.modelName} for ${lang.displayName}")

        var committedHistory = ""
        var lastDraftText = ""
        var lastDraftChangeMs = SystemClock.elapsedRealtime()
        val DRAFT_STALE_MS = 3_000L
        var streamCommittedPrefix = ""

        try {
            repository.markListening()
            val audioFlow = audioCaptureController.open(
                config = repository.currentConfig(),
                onRms = { rms -> overlayController.updateVolume(rms) },
            )

            val BATCH_SAMPLES = 16000 / 2
            val audioBatch = mutableListOf<Float>()
            val batchLock = Any()
            var totalSamplesFed = 0L
            val startTimeMs = SystemClock.elapsedRealtime()

            val collectorJob = launch {
                audioFlow.collect { chunk ->
                    synchronized(batchLock) {
                        for (s in chunk) audioBatch.add(s / 32768f)
                    }
                }
            }

            try {
                while (collectorJob.isActive && currentCoroutineContext().isActive) {
                    val batch: FloatArray?
                    synchronized(batchLock) {
                        if (audioBatch.size >= BATCH_SAMPLES) {
                            val maxBuffer = 16000 * 10
                            if (audioBatch.size > maxBuffer) {
                                audioBatch.subList(0, audioBatch.size - maxBuffer).clear()
                            }
                            batch = FloatArray(audioBatch.size) { audioBatch[it] }
                            audioBatch.clear()
                        } else {
                            batch = null
                        }
                    }

                    if (batch != null) {
                        stream.acceptWaveform(batch, 16000)
                        totalSamplesFed += batch.size
                        while (recognizer.isReady(stream)) { recognizer.decode(stream) }

                        val result = recognizer.getResult(stream)
                        val rawText = result.text.trim()
                        val text = if (streamCommittedPrefix.isNotEmpty()) {
                            if (rawText.startsWith(streamCommittedPrefix))
                                rawText.substring(streamCommittedPrefix.length).trimStart()
                            else { streamCommittedPrefix = ""; rawText }
                        } else rawText

                        val isEndpoint = recognizer.isEndpoint(stream)
                        if (isEndpoint) {
                            if (text.isNotBlank()) {
                                val toCommit = if (!lang.hasNativePunctuation ||
                                    !text.trimEnd().last().isSentencePunct()
                                ) "$text." else text
                                committedHistory = if (committedHistory.isEmpty()) toCommit
                                else "$committedHistory $toCommit"
                                Log.i("Sherpa", "COMMIT: '$toCommit'")
                            }
                            recognizer.reset(stream)
                            streamCommittedPrefix = ""
                            lastDraftText = ""
                            lastDraftChangeMs = SystemClock.elapsedRealtime()
                            repository.setTranscriptSegments(committedHistory, "", SystemClock.elapsedRealtime())
                        } else {
                            if (text != lastDraftText) { lastDraftText = text; lastDraftChangeMs = SystemClock.elapsedRealtime() }
                            val silenceMs = SystemClock.elapsedRealtime() - lastDraftChangeMs
                            val boundary = if (lang.hasNativePunctuation) splitAtSentenceBoundary(text) else null

                            if (boundary != null) {
                                val (before, after) = boundary
                                committedHistory = if (committedHistory.isEmpty()) before else "$committedHistory $before"
                                val afterTrimmed = after.trimStart()
                                streamCommittedPrefix = if (rawText.length >= afterTrimmed.length)
                                    rawText.substring(0, rawText.length - afterTrimmed.length).trimEnd() else rawText
                                lastDraftText = after; lastDraftChangeMs = SystemClock.elapsedRealtime()
                                Log.i("Sherpa", "CASE1: '$before' | draft='$after'")
                                repository.setTranscriptSegments(committedHistory, after, SystemClock.elapsedRealtime())
                            } else if (lang.hasNativePunctuation && text.trimEnd().lastOrNull()?.isSentencePunct() == true && silenceMs >= 600L) {
                                committedHistory = if (committedHistory.isEmpty()) text else "$committedHistory $text"
                                recognizer.reset(stream); streamCommittedPrefix = ""; lastDraftText = ""; lastDraftChangeMs = SystemClock.elapsedRealtime()
                                Log.i("Sherpa", "CASE2: '$text'")
                                repository.setTranscriptSegments(committedHistory, "", SystemClock.elapsedRealtime())
                            } else if (!lang.hasNativePunctuation && text.isNotBlank()) {
                                // Case 3: silence-based commit for models without native punctuation
                                // (ZH, RU, All8Lang). draftCommitThresholdMs() only computes the
                                // threshold — the caller MUST perform the actual commit here.
                                // ⚠️ EASY TO FORGET: showing the draft with a period is NOT a commit.
                                // Mirrors Windows run_streaming_loop Case 3 exactly:
                                //   committed_history += text + "."
                                //   stream_committed_prefix = raw_text  (strips committed part)
                                //   publish empty draft
                                val thresholdMs = draftCommitThresholdMs(text)
                                if (silenceMs >= thresholdMs || silenceMs >= DRAFT_STALE_MS) {
                                    val toCommit = "${text.trimEnd()}."
                                    committedHistory = if (committedHistory.isEmpty()) toCommit else "$committedHistory $toCommit"
                                    streamCommittedPrefix = rawText.trimEnd()
                                    lastDraftText = ""; lastDraftChangeMs = SystemClock.elapsedRealtime()
                                    Log.i("Sherpa", "CASE3: '$toCommit'")
                                    repository.setTranscriptSegments(committedHistory, "", SystemClock.elapsedRealtime())
                                } else {
                                    repository.setTranscriptSegments(committedHistory, text, SystemClock.elapsedRealtime())
                                }
                            } else {
                                val draft = if (text.isNotBlank() && silenceMs >= DRAFT_STALE_MS) "${text.trimEnd()}." else text
                                repository.setTranscriptSegments(committedHistory, draft, SystemClock.elapsedRealtime())
                            }
                        }

                        if (totalSamplesFed % (16000 * 2) < batch.size.toLong()) {
                            val audioMs = totalSamplesFed * 1000 / 16000; val wallMs = SystemClock.elapsedRealtime() - startTimeMs
                            Log.i("Sherpa", "fed=${audioMs/1000}s wall=${wallMs/1000}s lag=${wallMs - audioMs}ms")
                        }
                    } else {
                        delay(50)
                    }
                }
            } finally {
                collectorJob.cancel()
            }
        } finally {
            stream.release()
            recognizer.release()
        }
    }
}
