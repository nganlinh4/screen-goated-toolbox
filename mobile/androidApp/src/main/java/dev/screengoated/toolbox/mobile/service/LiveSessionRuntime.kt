package dev.screengoated.toolbox.mobile.service

import android.content.Context
import android.os.SystemClock
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

        sessionJob = scope.launch {
            repository.markStarting()

            if (useParakeet) {
                repository.setTranscriptionMethod(TranscriptionMethod.PARAKEET)
            } else {
                repository.setTranscriptionMethod(TranscriptionMethod.GEMINI_LIVE)
            }

            val translationJob = launch(Dispatchers.IO) {
                runTranslationLoop()
            }

            try {
                if (useParakeet) {
                    runParakeetSession()
                } else {
                    runGeminiSession(apiKey)
                }
            } catch (cancelled: CancellationException) {
                throw cancelled
            } catch (error: Throwable) {
                repository.fail(error.message ?: "Live transcription stopped unexpectedly.")
            } finally {
                translationJob.cancel()
            }
        }
    }

    private suspend fun runGeminiSession(apiKey: String) {
        withContext(Dispatchers.IO) {
            liveSocketClient.runSession(
                apiKey = apiKey,
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
            overlayController.showDownloadModal()
            try {
                withContext(Dispatchers.IO) {
                    // Launch state observer for progress updates
                    val progressJob = launch(Dispatchers.Main) {
                        parakeetModelManager.state.collect { state ->
                            when (state) {
                                is dev.screengoated.toolbox.mobile.service.parakeet.ParakeetModelState.Downloading -> {
                                    overlayController.updateDownloadProgress(state.progress, state.currentFile)
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
                repository.updateTranscriptionModel(RealtimeModelIds.TRANSCRIPTION_GEMINI)
                return
            }
        }

        withContext(Dispatchers.IO) {
            val modelDir = java.io.File(context.filesDir, "models/parakeet")
            val engine = ParakeetEngine(modelDir)
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

    private suspend fun runTranslationLoop() {
        while (currentCoroutineContext().isActive) {
            repository.forceCommitIfDue(SystemClock.elapsedRealtime())
            val nowMs = SystemClock.elapsedRealtime()
            if (nowMs - lastTranslationAttemptAtMs < TRANSLATION_INTERVAL_MS) {
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
                val usedProvider = translationClient.streamTranslation(
                    geminiApiKey = repository.currentApiKey(),
                    cerebrasApiKey = repository.currentCerebrasApiKey(),
                    request = request,
                    targetLanguage = repository.currentConfig().targetLanguage,
                    providerId = requestedProvider,
                    model = repository.currentConfig().translationProvider.model,
                    onDelta = { delta ->
                        repository.appendTranslationDelta(
                            text = delta,
                            nowMs = SystemClock.elapsedRealtime(),
                        )
                    },
                )
                // Only persist fallback switch if user hasn't changed the model during this request
                if (usedProvider != requestedProvider &&
                    repository.translationModelId() == requestedProvider
                ) {
                    repository.updateTranslationModel(usedProvider)
                }
                if (request.hasFinishedDelimiter) {
                    repository.finalizeTranslation(request.bytesToCommit)
                }
                maybeSpeakCommittedText()
                repository.updateMetrics(
                    repository.state.value.metrics.copy(
                        translationLatencyMs = SystemClock.elapsedRealtime() - startedAt,
                        lastUpdatedEpochMs = System.currentTimeMillis(),
                    ),
                )
                if (repository.state.value.phase != SessionPhase.ERROR) {
                    repository.markListening()
                }
            } catch (cancelled: CancellationException) {
                throw cancelled
            } catch (_: Throwable) {
                // Translation failure should not kill the session — retry next cycle
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
        realtimeTtsCoordinator.stopAndReset()
        if (!keepOverlay) {
            overlayController.hide()
        }
    }
}

private const val TRANSLATION_INTERVAL_MS = 1_500L

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
