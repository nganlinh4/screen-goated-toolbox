package dev.screengoated.toolbox.mobile.service

import android.content.Context
import android.os.SystemClock
import dev.screengoated.toolbox.mobile.capture.AudioCaptureController
import dev.screengoated.toolbox.mobile.model.AndroidLiveSessionRepository
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
    context: Context,
    private val repository: AndroidLiveSessionRepository,
    projectionConsentStore: ProjectionConsentStore,
    private val liveSocketClient: GeminiLiveSocketClient,
    private val translationClient: RealtimeTranslationClient,
    overlaySupported: Boolean,
    stopRequested: () -> Unit,
    sourceModeChanged: (SourceMode) -> Unit,
) {
    private var lastTranslationAttemptAtMs: Long = 0L
    private val audioCaptureController = AudioCaptureController(context, projectionConsentStore)
    private val textToSpeech = RealtimeTtsController(context)
    private val overlayController = OverlayController(
        context = context,
        repository = repository,
        overlaySupported = overlaySupported,
        stopRequested = stopRequested,
        restartRequested = { requestRestart() },
        sourceModeChanged = sourceModeChanged,
        stopTextToSpeech = { textToSpeech.stop() },
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
        textToSpeech.stopAndReset()
        overlayController.hide()
    }

    private fun requestRestart() {
        val scope = hostScope ?: return
        scope.launch {
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

        textToSpeech.stopAndReset()
        sessionJob = scope.launch {
            repository.markStarting()
            repository.setTranscriptionMethod(TranscriptionMethod.GEMINI_LIVE)

            val translationJob = launch(Dispatchers.IO) {
                runTranslationLoop()
            }

            try {
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
            } catch (cancelled: CancellationException) {
                throw cancelled
            } catch (error: Throwable) {
                repository.fail(error.message ?: "Live transcription stopped unexpectedly.")
            } finally {
                translationJob.cancel()
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
            try {
                val usedProvider = translationClient.streamTranslation(
                    geminiApiKey = repository.currentApiKey(),
                    cerebrasApiKey = repository.currentCerebrasApiKey(),
                    request = request,
                    targetLanguage = repository.currentConfig().targetLanguage,
                    providerId = repository.currentConfig().translationProvider.id,
                    model = repository.currentConfig().translationProvider.model,
                    onDelta = { delta ->
                        repository.appendTranslationDelta(
                            text = delta,
                            nowMs = SystemClock.elapsedRealtime(),
                        )
                    },
                )
                if (usedProvider != repository.translationModelId()) {
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
            } catch (error: Throwable) {
                repository.fail(error.message ?: "Translation failed.")
            }
            delay(100)
        }
    }

    private fun maybeSpeakCommittedText() {
        if (!overlayController.isTranslationVisible()) {
            textToSpeech.stop()
            return
        }
        val state = repository.state.value
        textToSpeech.speakCommittedText(
            committedText = state.liveText.committedTranslation,
            targetLanguage = state.config.targetLanguage,
            settings = repository.currentRealtimeTtsSettings(),
        )
    }

    private suspend fun stopSession(keepOverlay: Boolean) {
        sessionJob?.cancelAndJoin()
        sessionJob = null
        lastTranslationAttemptAtMs = 0L
        textToSpeech.stopAndReset()
        if (!keepOverlay) {
            overlayController.hide()
        }
    }
}

private const val TRANSLATION_INTERVAL_MS = 1_500L
