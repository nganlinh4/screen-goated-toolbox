package dev.screengoated.toolbox.mobile.service

import android.content.Context
import android.os.SystemClock
import android.util.Log
import dev.screengoated.toolbox.mobile.AppToastBus
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.capture.AudioCaptureController
import dev.screengoated.toolbox.mobile.model.AndroidLiveSessionRepository
import dev.screengoated.toolbox.mobile.model.RealtimeModelIds
import dev.screengoated.toolbox.mobile.service.tts.RealtimeTtsCoordinator
import dev.screengoated.toolbox.mobile.service.tts.TtsRuntimeService
import dev.screengoated.toolbox.mobile.shared.live.DisplayMode
import dev.screengoated.toolbox.mobile.shared.live.GeneratedLiveModelCatalog
import dev.screengoated.toolbox.mobile.shared.live.LiveTranslationModelCatalog
import dev.screengoated.toolbox.mobile.shared.live.LiveTranslateParity
import dev.screengoated.toolbox.mobile.shared.live.SessionPhase
import dev.screengoated.toolbox.mobile.shared.live.SourceMode
import dev.screengoated.toolbox.mobile.shared.live.TranscriptionMethod
import dev.screengoated.toolbox.mobile.storage.ProjectionConsentStore
import dev.screengoated.toolbox.mobile.ui.i18n.apiKeyErrorToastText
import dev.screengoated.toolbox.mobile.ui.i18n.uiLocalized
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.delay
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

class LiveSessionRuntime(
    internal val context: Context,
    internal val repository: AndroidLiveSessionRepository,
    projectionConsentStore: ProjectionConsentStore,
    internal val liveSocketClient: GeminiLiveSocketClient,
    internal val s2sClient: GeminiS2sClient,
    internal val translationClient: RealtimeTranslationClient,
    ttsRuntimeService: TtsRuntimeService,
    internal val toastBus: AppToastBus,
    overlaySupported: Boolean,
    stopRequested: () -> Unit,
    sourceModeChanged: (SourceMode) -> Unit,
) {
    internal var lastTranslationAttemptAtMs: Long = 0L
    internal var translationIntervalMs: Long = TRANSLATION_INTERVAL_MS
    internal val audioCaptureController = AudioCaptureController(context, projectionConsentStore)
    internal val realtimeTtsCoordinator = RealtimeTtsCoordinator(ttsRuntimeService)
    private val readReconcileLock = Any()
    internal val overlayController = OverlayController(
        context = context,
        repository = repository,
        overlaySupported = overlaySupported,
        stopRequested = stopRequested,
        restartRequested = { requestRestart() },
        sourceModeChanged = sourceModeChanged,
        stopTextToSpeech = { synchronized(readReconcileLock) { realtimeTtsCoordinator.stop() } },
        resetTextToSpeech = { synchronized(readReconcileLock) { realtimeTtsCoordinator.stopAndReset() } },
        setS2sPlaybackVisible = s2sClient::setPlaybackVisible,
        ttsRuntimeService = ttsRuntimeService,
    )

    @Volatile internal var sessionJob: Job? = null
    internal var hostScope: CoroutineScope? = null
    private val sessionEpoch = LiveSessionEpoch()

    fun start(scope: CoroutineScope) {
        hostScope = scope
        val epoch = sessionEpoch.advance()
        scope.launch {
            if (!stopSession(keepOverlay = false, expectedEpoch = epoch)) return@launch
            // Delay before creating hardware-accelerated overlay windows. When the
            // service is started by ProjectionConsentProxyActivity, the activity's
            // surface is still tearing down. Chrome GPU crashes with a null-deref if
            // we attach WebViews during that compositor transition.
            delay(600)
            if (!sessionEpoch.isCurrent(epoch)) return@launch
            launchSession(scope, preserveFrozenPrefix = false, expectedEpoch = epoch)
        }
    }

    fun stop() {
        sessionEpoch.advance()
        s2sClient.stopPlayback()
        sessionJob?.cancel()
        sessionJob = null
        lastTranslationAttemptAtMs = 0L
        translationIntervalMs = TRANSLATION_INTERVAL_MS
        synchronized(readReconcileLock) { realtimeTtsCoordinator.stopAndReset() }
        overlayController.hide()
    }

    fun dispose() = realtimeTtsCoordinator.close()

    internal fun requestRestart() {
        val scope = hostScope ?: return
        val epoch = sessionEpoch.advance()
        scope.launch {
            audioCaptureController.preserveConsentOnClose = true
            if (!stopSession(keepOverlay = true, expectedEpoch = epoch)) return@launch
            repository.freezeCurrentTranscript()
            launchSession(scope, preserveFrozenPrefix = true, expectedEpoch = epoch)
        }
    }

    internal suspend fun launchSession(
        scope: CoroutineScope,
        preserveFrozenPrefix: Boolean,
        expectedEpoch: Long,
    ) {
        if (!sessionEpoch.isCurrent(expectedEpoch)) return
        lastTranslationAttemptAtMs = 0L
        translationIntervalMs = TRANSLATION_INTERVAL_MS
        val config = repository.currentConfig()
        val apiKey = repository.currentApiKey()
        val modelId = config.transcriptionProvider.id
        val useGeminiS2s = RealtimeModelIds.isGeminiS2sModelId(modelId)

        // Transcription needs a Gemini key; translation always has the keyless GTX fallback.
        if (RealtimeModelIds.requiresGeminiApiKey(modelId) && apiKey.isBlank()) {
            apiKeyErrorToastText("NO_API_KEY:google", repository.currentUiPreferences().uiLanguage)
                ?.let(toastBus::show)
            repository.fail("Add your Gemini API key before starting live translate.")
            return
        }

        if (config.displayMode == DisplayMode.OVERLAY) {
            overlayController.show(scope)
        }

        realtimeTtsCoordinator.stopAndReset()
        if (RealtimeModelIds.isUnavailableOnAndroid(modelId)) {
            repository.fail("This transcription model is not available on Android.")
            return
        }
        Log.i(
            SESSION_TAG,
            "launch model_id=$modelId api_model=${config.transcriptionProvider.model} " +
                "s2s=$useGeminiS2s source=${config.sourceMode} " +
                "target=${config.targetLanguage}",
        )

        val newSession = scope.launch(start = CoroutineStart.LAZY) {
            if (!sessionEpoch.isCurrent(expectedEpoch)) return@launch
            val sessionOwner = currentCoroutineContext()[Job] ?: error("Session job unavailable")
            repository.markStarting(preserveFrozenPrefix = preserveFrozenPrefix)

            if (useGeminiS2s) {
                repository.setTranscriptionMethod(TranscriptionMethod.GEMINI_LIVE_S2S)
            } else if (
                GeneratedLiveModelCatalog.endpointProfile(config.transcriptionProvider.model)
                    ?.protocol == "live-transcribe"
            ) {
                repository.setTranscriptionMethod(TranscriptionMethod.GEMINI_TRANSCRIBE)
            } else {
                repository.setTranscriptionMethod(TranscriptionMethod.GEMINI_LIVE)
            }

            val translationJob = if (useGeminiS2s) {
                null
            } else {
                launch(Dispatchers.IO) {
                    runTranslationLoop()
                }
            }
            val readRevisionJob = if (useGeminiS2s) null else launch(Dispatchers.IO) {
                repository.state.map { it.liveText.translationRevision }.distinctUntilChanged().collect {
                    maybeSpeakCommittedText()
                }
            }

            try {
                if (useGeminiS2s) {
                    realtimeTtsCoordinator.stopAndReset()
                    runGeminiS2sSession(
                        apiKey = apiKey,
                        model = config.transcriptionProvider.model,
                    )
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
                if (sessionEpoch.isCurrent(expectedEpoch) && sessionJob === sessionOwner) {
                    repository.fail(error.message ?: "Live transcription stopped unexpectedly.")
                }
            } finally {
                translationJob?.cancel()
                readRevisionJob?.cancel()
            }
        }
        sessionJob = newSession
        newSession.start()
    }

    internal suspend fun runGeminiS2sSession(
        apiKey: String,
        model: String,
    ) {
        withContext(Dispatchers.IO) {
            val captureConfig = repository.currentConfig()
            Log.i(
                SESSION_TAG,
                "s2s-start model=$model source=${captureConfig.sourceMode} target=${captureConfig.targetLanguage}",
            )
            s2sClient.runSession(
                apiKey = apiKey,
                model = model,
                sourceMode = captureConfig.sourceMode,
                audioChunks = audioCaptureController.openMapped(
                    config = captureConfig,
                    onRms = { rms -> overlayController.updateVolume(rms) },
                    captureGeneration = s2sClient.visibilityGate::captureGeneration,
                ) {
                    val epoch = s2sClient.visibilityGate.snapshot()
                    val finish: (ShortArray) -> S2sCapturedAudio = { samples ->
                        S2sCapturedAudio(samples, epoch.takeIf(s2sClient.visibilityGate::isCurrent))
                    }
                    finish
                },
                settingsProvider = {
                    val config = repository.currentConfig()
                    val global = repository.currentGlobalTtsSettings()
                    val custom = global.languageConditions.firstOrNull {
                        it.languageName.equals(config.targetLanguage, ignoreCase = true) ||
                            it.languageCode.equals(
                                dev.screengoated.toolbox.mobile.model.LanguageCatalog.codeForName(config.targetLanguage),
                                ignoreCase = true,
                            )
                    }?.instruction.orEmpty()
                    GeminiS2sRuntimeSettings(
                        targetLanguage = config.targetLanguage,
                        customInstruction = custom,
                        globalTts = global,
                        realtime = repository.currentRealtimeTtsSettings(),
                    )
                },
                onDisplay = { snapshot ->
                    repository.markListening()
                    repository.setGeminiS2sDisplay(
                        sourceCommitted = snapshot.sourceCommitted,
                        sourceDraft = snapshot.sourceDraft,
                        targetCommitted = snapshot.targetCommitted,
                        targetDraft = snapshot.targetDraft,
                        nowMs = SystemClock.elapsedRealtime(),
                    )
                },
            )
            Log.i(SESSION_TAG, "s2s-exit")
        }
    }

    internal suspend fun runGeminiSession(
        apiKey: String,
        model: String,
    ) {
        withContext(Dispatchers.IO) {
            GeminiTranscribeVocabulary.replace(repository.currentConfig().customVocabulary)
            val usesInterimTranscripts =
                GeneratedLiveModelCatalog.endpointProfile(model)?.protocol == "live-transcribe"
            val delivery = dev.screengoated.toolbox.mobile.shared.live.TranscriptionDelivery()
            liveSocketClient.runSession(
                apiKey = apiKey,
                model = model,
                audioChunks = audioCaptureController.open(
                    config = repository.currentConfig(),
                    onRms = { rms -> overlayController.updateVolume(rms) },
                ),
                onAudioGap = { toastBus.show(context.uiLocalized().getString(R.string.live_audio_gap)) },
                onTranscript = { transcript, isFinal ->
                    repository.markListening()
                    if (usesInterimTranscripts) {
                        delivery.update(transcript, isFinal)
                        repository.setTranscriptSegments(
                            committed = delivery.committed,
                            draft = delivery.interim,
                            nowMs = SystemClock.elapsedRealtime(),
                        )
                    } else {
                        repository.appendTranscript(
                            text = transcript,
                            nowMs = SystemClock.elapsedRealtime(),
                        )
                    }
                },
            )
        }
    }


    internal suspend fun runTranslationLoop() {
        while (currentCoroutineContext().isActive) {
            val nowMs = SystemClock.elapsedRealtime()
            if (repository.forceCommitIfDue(nowMs)) {
                lastTranslationAttemptAtMs = (nowMs - translationIntervalMs).coerceAtLeast(0L)
            }
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
                var result = translationClient.translate(
                    geminiApiKey = repository.currentApiKey(),
                    groqApiKey = repository.currentGroqApiKey(),
                    nvidiaApiKey = repository.currentNvidiaApiKey(),
                    request = request,
                    targetLanguage = repository.currentConfig().targetLanguage,
                    providerId = requestedProvider,
                    llmChain = repository.currentTextToTextChain(),
                    runtimeSettings = repository.currentPresetRuntimeSettings(),
                    isCurrent = { LiveTranslateParity.isTranslationRequestCurrent(repository.state.value.liveText, request) },
                )
                ensureTranslationRequestCurrent { LiveTranslateParity.isTranslationRequestCurrent(repository.state.value.liveText, request) }
                var usedProvider = result.providerId
                var applied = repository.applyTranslationResponse(
                    request = request,
                    response = result.response,
                    nowMs = SystemClock.elapsedRealtime(),
                )
                if (!applied && !LiveTranslateParity.isTranslationRequestCurrent(repository.state.value.liveText, request)) {
                    lastTranslationAttemptAtMs = 0L
                    continue
                }
                if (!applied && usedProvider == requestedProvider) {
                    val fallbackProvider = fallbackTranslationProviderId(requestedProvider)
                    result = translationClient.translateWithExactProvider(
                        geminiApiKey = repository.currentApiKey(),
                        groqApiKey = repository.currentGroqApiKey(),
                        nvidiaApiKey = repository.currentNvidiaApiKey(),
                        request = request,
                        targetLanguage = repository.currentConfig().targetLanguage,
                        providerId = fallbackProvider,
                        llmChain = repository.currentTextToTextChain(),
                        runtimeSettings = repository.currentPresetRuntimeSettings(),
                        isCurrent = { LiveTranslateParity.isTranslationRequestCurrent(repository.state.value.liveText, request) },
                    )
                    usedProvider = result.providerId
                    applied = repository.applyTranslationResponse(
                        request = request,
                        response = result.response,
                        nowMs = SystemClock.elapsedRealtime(),
                    )
                }
                if (!applied) {
                    error("Translation response was rejected by the current transcript state.")
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
            } catch (_: StaleTranslationRequestException) {
                currentCoroutineContext().ensureActive()
                lastTranslationAttemptAtMs = 0L
            } catch (error: Throwable) {
                currentCoroutineContext().ensureActive()
                if (!LiveTranslateParity.isTranslationRequestCurrent(repository.state.value.liveText, request)) {
                    lastTranslationAttemptAtMs = 0L
                    continue
                }
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

    internal fun maybeSpeakCommittedText() = synchronized(readReconcileLock) {
        if (sessionJob?.isActive != true) return@synchronized
        if (!overlayController.isTranslationVisible()) {
            realtimeTtsCoordinator.stop()
            return@synchronized
        }
        val state = repository.state.value
        realtimeTtsCoordinator.update(
            committedText = state.liveText.committedTranslation,
            targetLanguage = state.config.targetLanguage,
            globalSettings = repository.currentGlobalTtsSettings(),
            realtimeSettings = repository.currentRealtimeTtsSettings(),
            translationVisible = true,
            translationRevision = state.liveText.translationRevision,
        )
    }

    internal suspend fun stopSession(
        keepOverlay: Boolean, expectedJob: Job? = sessionJob, expectedEpoch: Long = sessionEpoch.current(),
    ): Boolean {
        if (!sessionEpoch.isCurrent(expectedEpoch) || sessionJob !== expectedJob) return false
        s2sClient.stopPlayback()
        return awaitLiveSessionStop(expectedJob, isCurrent = { sessionEpoch.isCurrent(expectedEpoch) && sessionJob === it }) {
            sessionJob = null
            lastTranslationAttemptAtMs = 0L
            translationIntervalMs = TRANSLATION_INTERVAL_MS
            realtimeTtsCoordinator.stopAndReset()
            if (!keepOverlay) {
                overlayController.hide()
            }
        }
    }
}

internal const val TRANSLATION_INTERVAL_MS = 1_500L
internal const val TRANSLATION_INTERVAL_MAX_MS = 4_000L
internal const val TRANSLATION_TAG = "LiveTranslate"
internal const val SESSION_TAG = "LiveSessionRuntime"

internal fun fallbackTranslationProviderId(providerId: String): String {
    return if (providerId == LiveTranslationModelCatalog.PROVIDER_GTX) {
        LiveTranslationModelCatalog.PROVIDER_LLM
    } else {
        LiveTranslationModelCatalog.PROVIDER_GTX
    }
}

internal fun computeAdaptiveTranslationIntervalMs(latencyMs: Long): Long {
    return (latencyMs + 250L)
        .coerceAtLeast(TRANSLATION_INTERVAL_MS)
        .coerceAtMost(TRANSLATION_INTERVAL_MAX_MS)
}
