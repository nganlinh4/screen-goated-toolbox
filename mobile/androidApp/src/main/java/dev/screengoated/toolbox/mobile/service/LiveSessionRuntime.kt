package dev.screengoated.toolbox.mobile.service

import android.content.Context
import android.os.SystemClock
import android.util.Log
import dev.screengoated.toolbox.mobile.capture.AudioCaptureController
import dev.screengoated.toolbox.mobile.model.AndroidLiveSessionRepository
import dev.screengoated.toolbox.mobile.model.RealtimeModelIds
import dev.screengoated.toolbox.mobile.service.parakeet.ParakeetEngine
import dev.screengoated.toolbox.mobile.service.parakeet.ParakeetModelManager
import dev.screengoated.toolbox.mobile.service.tts.RealtimeTtsCoordinator
import dev.screengoated.toolbox.mobile.service.tts.TtsRuntimeService
import dev.screengoated.toolbox.mobile.shared.live.DisplayMode
import dev.screengoated.toolbox.mobile.shared.live.SessionPhase
import dev.screengoated.toolbox.mobile.shared.live.SourceMode
import dev.screengoated.toolbox.mobile.shared.live.TranscriptionMethod
import dev.screengoated.toolbox.mobile.storage.ProjectionConsentStore
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

class LiveSessionRuntime(
    private val context: Context,
    private val repository: AndroidLiveSessionRepository,
    projectionConsentStore: ProjectionConsentStore,
    private val liveSocketClient: GeminiLiveSocketClient,
    private val translationClient: RealtimeTranslationClient,
    ttsRuntimeService: TtsRuntimeService,
    overlaySupported: Boolean,
    stopRequested: () -> Unit,
    sourceModeChanged: (SourceMode) -> Unit,
    val parakeetModelManager: ParakeetModelManager,
) {
    private var lastTranslationAttemptAtMs: Long = 0L
    private var translationIntervalMs: Long = TRANSLATION_INTERVAL_MS
    private val audioCaptureController = AudioCaptureController(context, projectionConsentStore)
    private val realtimeTtsCoordinator = RealtimeTtsCoordinator(ttsRuntimeService)
    private val overlayController = OverlayController(
        context = context,
        repository = repository,
        overlaySupported = overlaySupported,
        stopRequested = stopRequested,
        restartRequested = { requestRestart() },
        sourceModeChanged = sourceModeChanged,
        stopTextToSpeech = { realtimeTtsCoordinator.stop() },
        ttsRuntimeService = ttsRuntimeService,
    )

    private var sessionJob: Job? = null
    private var hostScope: CoroutineScope? = null

    fun start(scope: CoroutineScope) {
        hostScope = scope
        scope.launch {
            stopSession(keepOverlay = false)
            launchSession(scope)
        }
    }

    fun stop() {
        sessionJob?.cancel()
        sessionJob = null
        lastTranslationAttemptAtMs = 0L
        translationIntervalMs = TRANSLATION_INTERVAL_MS
        realtimeTtsCoordinator.stopAndReset()
        overlayController.hide()
    }

    private fun requestRestart() {
        val scope = hostScope ?: return
        scope.launch {
            audioCaptureController.preserveConsentOnClose = true
            stopSession(keepOverlay = true)
            launchSession(scope)
        }
    }

    private suspend fun launchSession(scope: CoroutineScope) {
        lastTranslationAttemptAtMs = 0L
        translationIntervalMs = TRANSLATION_INTERVAL_MS
        val config = repository.currentConfig()
        val apiKey = repository.currentApiKey()
        if (apiKey.isBlank()) {
            repository.fail("Add your Gemini API key before starting live translate.")
            return
        }

        if (config.displayMode == DisplayMode.OVERLAY) {
            overlayController.show(scope)
        }

        realtimeTtsCoordinator.stopAndReset()
        val useParakeet = config.transcriptionProvider.id == RealtimeModelIds.TRANSCRIPTION_PARAKEET
        val modelId = config.transcriptionProvider.id
        val useMoonshine = modelId.startsWith("moonshine-") || modelId == "zipformer"
            || modelId == RealtimeModelIds.TRANSCRIPTION_MOONSHINE

        sessionJob = scope.launch {
            repository.markStarting()

            if (useParakeet) {
                repository.setTranscriptionMethod(TranscriptionMethod.PARAKEET)
            } else if (useMoonshine) {
                repository.setTranscriptionMethod(TranscriptionMethod.MOONSHINE)
            } else {
                repository.setTranscriptionMethod(TranscriptionMethod.GEMINI_LIVE)
            }

            val translationJob = launch(Dispatchers.IO) {
                runTranslationLoop()
            }

            try {
                if (useParakeet) {
                    runParakeetSession()
                } else if (useMoonshine) {
                    runMoonshineSession()
                } else {
                    runGeminiSession(
                        apiKey = apiKey,
                        model = config.transcriptionProvider.model,
                    )
                }
            } catch (cancelled: CancellationException) {
                throw cancelled
            } catch (error: Throwable) {
                Log.e("LiveSessionRuntime", "Transcription error", error)
                repository.fail(error.message ?: "Live transcription stopped unexpectedly.")
            } finally {
                translationJob.cancel()
            }
        }
    }

    private suspend fun runGeminiSession(
        apiKey: String,
        model: String,
    ) {
        withContext(Dispatchers.IO) {
            liveSocketClient.runSession(
                apiKey = apiKey,
                model = model,
                audioChunks = audioCaptureController.open(
                    config = repository.currentConfig(),
                    onRms = { rms -> overlayController.updateVolume(rms) },
                ),
                onTranscript = { transcript ->
                    repository.markListening()
                    repository.appendTranscript(
                        text = transcript,
                        nowMs = SystemClock.elapsedRealtime(),
                    )
                },
            )
        }
    }

    private suspend fun runParakeetSession() {
        if (!parakeetModelManager.isInstalled()) {
            overlayController.showDownloadModal("Parakeet")
            try {
                withContext(Dispatchers.IO) {
                    // Launch state observer for progress updates
                    val progressJob = launch(Dispatchers.Main) {
                        parakeetModelManager.state.collect { state ->
                            when (state) {
                                is dev.screengoated.toolbox.mobile.service.parakeet.ParakeetModelState.Downloading -> {
                                    overlayController.updateDownloadProgress(state.progress * 100, state.currentFile)
                                }
                                else -> {}
                            }
                        }
                    }
                    try {
                        parakeetModelManager.download()
                    } finally {
                        progressJob.cancel()
                    }
                }
                overlayController.hideDownloadModal()
            } catch (e: CancellationException) {
                overlayController.hideDownloadModal()
                throw e
            }

            if (!parakeetModelManager.isInstalled()) {
                repository.updateTranscriptionModel(RealtimeModelIds.TRANSCRIPTION_GEMINI_2_5)
                return
            }
        }

        withContext(Dispatchers.IO) {
            val modelDir = java.io.File(context.filesDir, "models/parakeet")
            val engine = ParakeetEngine(modelDir, parakeetModelManager.ortLibDir())
            try {
                repository.markListening()
                val audioFlow = audioCaptureController.open(
                    config = repository.currentConfig(),
                    onRms = { rms -> overlayController.updateVolume(rms) },
                )

                val sampleAccumulator = mutableListOf<Float>()

                audioFlow.collect { chunk ->
                    for (s in chunk) {
                        sampleAccumulator.add(s / 32768f)
                    }

                    while (sampleAccumulator.size >= PARAKEET_CHUNK_SIZE) {
                        val chunkFloats = FloatArray(PARAKEET_CHUNK_SIZE)
                        for (i in 0 until PARAKEET_CHUNK_SIZE) {
                            chunkFloats[i] = sampleAccumulator.removeFirst()
                        }

                        val rawText = engine.transcribe(chunkFloats)
                        if (rawText.isNotEmpty()) {
                            val processed = processSentencePieceText(rawText)
                            if (processed.isNotEmpty()) {
                                repository.appendTranscript(
                                    text = processed,
                                    nowMs = SystemClock.elapsedRealtime(),
                                )
                            }
                        }
                    }
                }

                // Flush: send 3 silence chunks to extract any remaining text
                val silence = FloatArray(PARAKEET_CHUNK_SIZE)
                repeat(3) {
                    val rawText = engine.transcribe(silence)
                    if (rawText.isNotEmpty()) {
                        val processed = processSentencePieceText(rawText)
                        if (processed.isNotEmpty()) {
                            repository.appendTranscript(
                                text = processed,
                                nowMs = SystemClock.elapsedRealtime(),
                            )
                        }
                    }
                }
            } finally {
                engine.close()
            }
        }
    }

    private suspend fun runMoonshineSession() {
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
                } catch (e: CancellationException) {
                    overlayController.hideDownloadModal()
                    throw e
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
            } catch (e: CancellationException) {
                overlayController.hideDownloadModal()
                throw e
            }
            if (!moonshineManager.isInstalled(lang)) {
                repository.updateTranscriptionModel(RealtimeModelIds.TRANSCRIPTION_GEMINI_2_5)
                return
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

    private suspend fun runSherpaSession(
        lang: dev.screengoated.toolbox.mobile.service.moonshine.ZipformerLanguage,
        modelManager: dev.screengoated.toolbox.mobile.service.moonshine.MoonshineModelManager,
    ) {
        withContext(Dispatchers.IO) {
            val modelDir = modelManager.zipformerDir(lang).absolutePath

            val config = com.k2fsa.sherpa.onnx.OnlineRecognizerConfig(
                modelConfig = com.k2fsa.sherpa.onnx.OnlineModelConfig(
                    transducer = com.k2fsa.sherpa.onnx.OnlineTransducerModelConfig(
                        encoder = "$modelDir/${lang.sherpaEncoder()}",
                        decoder = "$modelDir/${lang.sherpaDecoder()}",
                        joiner = "$modelDir/${lang.sherpaJoiner()}",
                    ),
                    tokens = "$modelDir/tokens.txt",
                    modelType = lang.sherpaModelType,
                    numThreads = 2,
                ),
                enableEndpoint = true,
                decodingMethod = "greedy_search",
            )

            val recognizer = com.k2fsa.sherpa.onnx.OnlineRecognizer(config = config)
            // Check if native recognizer was created (ptr == 0 means failure)
            try {
                val ptrField = recognizer.javaClass.getDeclaredField("ptr")
                ptrField.isAccessible = true
                if (ptrField.getLong(recognizer) == 0L) {
                    Log.e("Sherpa", "Failed to create recognizer — model files missing or invalid config")
                    Log.e("Sherpa", "Model dir: $modelDir")
                    Log.e("Sherpa", "Encoder: ${lang.sherpaEncoder()}")
                    return@withContext
                }
            } catch (_: Exception) {}
            val stream = recognizer.createStream()
            Log.i("Sherpa", "Loaded ${lang.modelName} for ${lang.displayName}")

            var committedHistory = ""
            var lastDraftText = ""
            var lastDraftChangeMs = SystemClock.elapsedRealtime()
            val DRAFT_STALE_MS = 3_000L

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
                                    val drop = audioBatch.size - maxBuffer
                                    audioBatch.subList(0, drop).clear()
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

                            while (recognizer.isReady(stream)) {
                                recognizer.decode(stream)
                            }

                            val result = recognizer.getResult(stream)
                            val text = result.text.trim()

                            // Check for endpoint (utterance boundary)
                            val isEndpoint = recognizer.isEndpoint(stream)
                            if (isEndpoint) {
                                if (text.isNotBlank()) {
                                    // Zipformer has no punctuation — add period at
                                    // endpoint to mark sentence boundary for translation
                                    committedHistory = if (committedHistory.isEmpty()) {
                                        "$text."
                                    } else {
                                        "$committedHistory $text."
                                    }
                                    Log.i("Sherpa", "COMMIT: '$text'")
                                }
                                recognizer.reset(stream)
                                lastDraftText = ""
                                lastDraftChangeMs = SystemClock.elapsedRealtime()
                                repository.setTranscriptSegments(
                                    committed = committedHistory,
                                    draft = "",
                                    nowMs = SystemClock.elapsedRealtime(),
                                )
                            } else {
                                // Track draft changes for stale detection
                                if (text != lastDraftText) {
                                    lastDraftText = text
                                    lastDraftChangeMs = SystemClock.elapsedRealtime()
                                }
                                // If draft hasn't changed for 3s, add period to
                                // force translation commit (matching Windows pattern)
                                val draftToPublish = if (text.isNotBlank()
                                    && SystemClock.elapsedRealtime() - lastDraftChangeMs >= DRAFT_STALE_MS
                                ) {
                                    "${text.trimEnd()}."
                                } else {
                                    text
                                }
                                repository.setTranscriptSegments(
                                    committed = committedHistory,
                                    draft = draftToPublish,
                                    nowMs = SystemClock.elapsedRealtime(),
                                )
                            }

                            val audioTimeMs = totalSamplesFed * 1000 / 16000
                            val wallTimeMs = SystemClock.elapsedRealtime() - startTimeMs
                            val lagMs = wallTimeMs - audioTimeMs
                            if (totalSamplesFed % (16000 * 2) < batch.size.toLong()) {
                                Log.i("Sherpa", "fed=${audioTimeMs/1000}s wall=${wallTimeMs/1000}s lag=${lagMs}ms")
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

    private suspend fun runTranslationLoop() {
        while (currentCoroutineContext().isActive) {
            repository.forceCommitIfDue(SystemClock.elapsedRealtime())
            val nowMs = SystemClock.elapsedRealtime()
            if (nowMs - lastTranslationAttemptAtMs < translationIntervalMs) {
                maybeSpeakCommittedText()
                delay(100)
                continue
            }
            if (!overlayController.isTranslationVisible()) {
                lastTranslationAttemptAtMs = nowMs
                delay(500)
                continue
            }
            val request = repository.claimTranslationRequest()
            if (request == null) {
                lastTranslationAttemptAtMs = nowMs
                maybeSpeakCommittedText()
                delay(100)
                continue
            }

            lastTranslationAttemptAtMs = nowMs
            repository.markTranslating()
            val startedAt = SystemClock.elapsedRealtime()
            val requestedProvider = repository.currentConfig().translationProvider.id
            try {
                val result = translationClient.translate(
                    geminiApiKey = repository.currentApiKey(),
                    cerebrasApiKey = repository.currentCerebrasApiKey(),
                    request = request,
                    targetLanguage = repository.currentConfig().targetLanguage,
                    providerId = requestedProvider,
                    model = repository.currentConfig().translationProvider.model,
                )
                val usedProvider = result.providerId
                repository.applyTranslationResponse(
                    request = request,
                    response = result.response,
                    nowMs = SystemClock.elapsedRealtime(),
                )
                // Only persist fallback switch if user hasn't changed the model during this request
                if (usedProvider != requestedProvider &&
                    repository.translationModelId() == requestedProvider
                ) {
                    repository.updateTranslationModel(usedProvider)
                }
                val latencyMs = SystemClock.elapsedRealtime() - startedAt
                translationIntervalMs = computeAdaptiveTranslationIntervalMs(latencyMs)
                Log.d(
                    TRANSLATION_TAG,
                    "success provider=$usedProvider range=${request.sourceStart}-${request.sourceEnd} latency=${latencyMs}ms nextInterval=${translationIntervalMs}ms",
                )
                maybeSpeakCommittedText()
                repository.updateMetrics(
                    repository.state.value.metrics.copy(
                        translationLatencyMs = latencyMs,
                        lastUpdatedEpochMs = System.currentTimeMillis(),
                    ),
                )
                if (repository.state.value.phase != SessionPhase.ERROR) {
                    repository.markListening()
                }
            } catch (cancelled: CancellationException) {
                throw cancelled
            } catch (error: Throwable) {
                // Translation failure should not kill the session — retry next cycle
                translationIntervalMs = (translationIntervalMs + 250L).coerceAtMost(TRANSLATION_INTERVAL_MAX_MS)
                Log.w(
                    TRANSLATION_TAG,
                    "failure provider=$requestedProvider range=${request.sourceStart}-${request.sourceEnd} nextInterval=${translationIntervalMs}ms",
                    error,
                )
                if (repository.state.value.phase != SessionPhase.ERROR) {
                    repository.markListening()
                }
            }
            delay(100)
        }
    }

    private fun maybeSpeakCommittedText() {
        if (!overlayController.isTranslationVisible()) {
            realtimeTtsCoordinator.stop()
            return
        }
        val state = repository.state.value
        realtimeTtsCoordinator.update(
            committedText = state.liveText.committedTranslation,
            targetLanguage = state.config.targetLanguage,
            globalSettings = repository.currentGlobalTtsSettings(),
            realtimeSettings = repository.currentRealtimeTtsSettings(),
            translationVisible = true,
        )
    }

    private suspend fun stopSession(keepOverlay: Boolean) {
        sessionJob?.cancelAndJoin()
        sessionJob = null
        lastTranslationAttemptAtMs = 0L
        translationIntervalMs = TRANSLATION_INTERVAL_MS
        realtimeTtsCoordinator.stopAndReset()
        if (!keepOverlay) {
            overlayController.hide()
        }
    }
}

private const val TRANSLATION_INTERVAL_MS = 1_500L
private const val TRANSLATION_INTERVAL_MAX_MS = 4_000L
private const val TRANSLATION_TAG = "LiveTranslate"

/** 160ms chunk at 16kHz = 2560 samples (matches Windows parakeet-rs) */
private const val PARAKEET_CHUNK_SIZE = 2560

/**
 * Matches Windows `process_sentencepiece_text()` in parakeet.rs:
 * Preserves leading space (word boundary indicator ▁) while cleaning up the token.
 */
private fun processSentencePieceText(text: String): String {
    val startsWithWord = text.startsWith('\u2581') || text.startsWith('▁')
    val processed = text.replace("\u2581", " ").replace("▁", " ").trim()
    if (processed.isEmpty()) return ""
    return if (startsWithWord) " $processed" else processed
}

private fun computeAdaptiveTranslationIntervalMs(latencyMs: Long): Long {
    return (latencyMs + 250L)
        .coerceAtLeast(TRANSLATION_INTERVAL_MS)
        .coerceAtMost(TRANSLATION_INTERVAL_MAX_MS)
}
