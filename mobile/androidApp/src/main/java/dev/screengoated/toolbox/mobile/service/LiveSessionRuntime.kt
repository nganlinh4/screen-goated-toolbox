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
    internal val overlayController = OverlayController(
        context = context,
        repository = repository,
        overlaySupported = overlaySupported,
        stopRequested = stopRequested,
        cancelDownloadRequested = { cancelActiveDownload() },
        restartRequested = { requestRestart() },
        sourceModeChanged = sourceModeChanged,
        stopTextToSpeech = { realtimeTtsCoordinator.stop() },
        ttsRuntimeService = ttsRuntimeService,
    )

    internal var sessionJob: Job? = null
    internal var hostScope: CoroutineScope? = null
    internal var downloadCancelAction: (() -> Unit)? = null
    internal var cancelInFlight = false

    fun start(scope: CoroutineScope) {
        hostScope = scope
        scope.launch {
            stopSession(keepOverlay = false)
            // Delay before creating hardware-accelerated overlay WebViews. When the
            // service is started by ProjectionConsentProxyActivity, the activity's
            // surface is still tearing down. Chrome GPU crashes with a null-deref if
            // we create LAYER_TYPE_HARDWARE WebViews during that compositor transition.
            delay(600)
            launchSession(scope, preserveFrozenPrefix = false)
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

    internal fun requestRestart() {
        val scope = hostScope ?: return
        scope.launch {
            audioCaptureController.preserveConsentOnClose = true
            stopSession(keepOverlay = true)
            repository.freezeCurrentTranscript()
            launchSession(scope, preserveFrozenPrefix = true)
        }
    }

    internal suspend fun launchSession(
        scope: CoroutineScope,
        preserveFrozenPrefix: Boolean,
    ) {
        lastTranslationAttemptAtMs = 0L
        translationIntervalMs = TRANSLATION_INTERVAL_MS
        val config = repository.currentConfig()
        val apiKey = repository.currentApiKey()
        if (apiKey.isBlank()) {
            apiKeyErrorToastText("NO_API_KEY:google", repository.currentUiPreferences().uiLanguage)
                ?.let(toastBus::show)
            repository.fail("Add your Gemini API key before starting live translate.")
            return
        }

        if (config.displayMode == DisplayMode.OVERLAY) {
            overlayController.show(scope)
        }

        realtimeTtsCoordinator.stopAndReset()
        if (config.transcriptionProvider.id == RealtimeModelIds.TRANSCRIPTION_PARAKEET) {
            repository.fail("Parakeet is visible for Windows parity but is not available on Android yet.")
            return
        }
        val modelId = repository.currentConfig().transcriptionProvider.id
        val useGeminiS2s = modelId == RealtimeModelIds.TRANSCRIPTION_GEMINI_S2S
        val useMoonshine = modelId.startsWith("moonshine-") || modelId == "zipformer"
            || modelId == RealtimeModelIds.TRANSCRIPTION_MOONSHINE
        Log.i(
            SESSION_TAG,
            "launch model_id=$modelId api_model=${repository.currentConfig().transcriptionProvider.model} " +
                "s2s=$useGeminiS2s moonshine=$useMoonshine source=${repository.currentConfig().sourceMode} " +
                "target=${repository.currentConfig().targetLanguage}",
        )

        sessionJob = scope.launch {
            repository.markStarting(preserveFrozenPrefix = preserveFrozenPrefix)

            // Auto-download native runtimes on demand (all engines).
            if (useMoonshine) {
                val nativeLibMgr = dev.screengoated.toolbox.mobile.service.nativelibs.NativeLibManager(context)
                downloadCancelAction = { nativeLibMgr.cancelAllDownloads() }
                val isMoonshineModel = useMoonshine && (modelId.startsWith("moonshine-") || modelId == "moonshine")
                val isZipformer = useMoonshine && (modelId == "zipformer" || modelId.startsWith("zipformer-"))

                // Determine which engines are needed
                val engines = mutableListOf<dev.screengoated.toolbox.mobile.service.nativelibs.NativeLibManager.Engine>()
                if (isMoonshineModel) {
                    engines.add(dev.screengoated.toolbox.mobile.service.nativelibs.NativeLibManager.Engine.ORT)
                }
                if (isMoonshineModel) {
                    engines.add(dev.screengoated.toolbox.mobile.service.nativelibs.NativeLibManager.Engine.MOONSHINE)
                }
                if (isZipformer) {
                    engines.add(dev.screengoated.toolbox.mobile.service.nativelibs.NativeLibManager.Engine.SHERPA)
                }

                // Download any missing engines
                val missing = engines.filter { !nativeLibMgr.isInstalled(it) }
                if (missing.isNotEmpty()) {
                    overlayController.showDownloadModal("ASR Runtime")
                    try {
                        for (engine in missing) {
                            val flow = nativeLibMgr.status(engine)
                            nativeLibMgr.startDownload(engine)
                            val progressJob = launch(Dispatchers.Main) {
                                flow.collect { status ->
                                    if (status is dev.screengoated.toolbox.mobile.service.nativelibs.NativeLibManager.Status.Downloading) {
                                        overlayController.updateDownloadProgress(
                                            status.progress * 100,
                                            engine.name,
                                        )
                                    }
                                }
                            }
                            withContext(Dispatchers.IO) {
                                while (
                                    flow.value !is dev.screengoated.toolbox.mobile.service.nativelibs.NativeLibManager.Status.Installed &&
                                    flow.value !is dev.screengoated.toolbox.mobile.service.nativelibs.NativeLibManager.Status.Error
                                ) {
                                    kotlinx.coroutines.delay(200)
                                }
                            }
                            progressJob.cancel()
                            if (flow.value is dev.screengoated.toolbox.mobile.service.nativelibs.NativeLibManager.Status.Error) {
                                overlayController.hideDownloadModal()
                                repository.fail("Failed to download ${engine.name} runtime.")
                                return@launch
                            }
                        }
                        overlayController.hideDownloadModal()
                        downloadCancelAction = null
                    } catch (e: kotlinx.coroutines.CancellationException) {
                        overlayController.hideDownloadModal()
                        downloadCancelAction = null
                        throw e
                    }
                }

                // Always load before session
                if (!nativeLibMgr.loadEngines(*engines.toTypedArray())) {
                    repository.fail("Failed to load ASR runtime libraries.")
                    return@launch
                }
            }

            if (useGeminiS2s) {
                repository.setTranscriptionMethod(TranscriptionMethod.GEMINI_LIVE_S2S)
            } else if (useMoonshine) {
                repository.setTranscriptionMethod(TranscriptionMethod.MOONSHINE)
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

            try {
                if (useGeminiS2s) {
                    realtimeTtsCoordinator.stopAndReset()
                    runGeminiS2sSession(
                        apiKey = apiKey,
                        model = config.transcriptionProvider.model,
                    )
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
                translationJob?.cancel()
            }
        }
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
                audioChunks = audioCaptureController.open(
                    config = captureConfig,
                    onRms = { rms -> overlayController.updateVolume(rms) },
                ),
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
                    cerebrasApiKey = repository.currentCerebrasApiKey(),
                    groqApiKey = repository.currentGroqApiKey(),
                    request = request,
                    targetLanguage = repository.currentConfig().targetLanguage,
                    providerId = requestedProvider,
                    llmChain = repository.currentTextToTextChain(),
                    runtimeSettings = repository.currentPresetRuntimeSettings(),
                )
                var usedProvider = result.providerId
                var applied = repository.applyTranslationResponse(
                    request = request,
                    response = result.response,
                    nowMs = SystemClock.elapsedRealtime(),
                )
                if (!applied && usedProvider == requestedProvider) {
                    val fallbackProvider = fallbackTranslationProviderId(requestedProvider)
                    result = translationClient.translateWithExactProvider(
                        geminiApiKey = repository.currentApiKey(),
                        cerebrasApiKey = repository.currentCerebrasApiKey(),
                        groqApiKey = repository.currentGroqApiKey(),
                        request = request,
                        targetLanguage = repository.currentConfig().targetLanguage,
                        providerId = fallbackProvider,
                        llmChain = repository.currentTextToTextChain(),
                        runtimeSettings = repository.currentPresetRuntimeSettings(),
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

    internal fun maybeSpeakCommittedText() {
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

    internal suspend fun stopSession(keepOverlay: Boolean) {
        sessionJob?.cancelAndJoin()
        sessionJob = null
        downloadCancelAction = null
        cancelInFlight = false
        lastTranslationAttemptAtMs = 0L
        translationIntervalMs = TRANSLATION_INTERVAL_MS
        realtimeTtsCoordinator.stopAndReset()
        if (!keepOverlay) {
            overlayController.hide()
        }
    }

    internal fun cancelActiveDownload() {
        if (cancelInFlight) {
            return
        }
        val cancelAction = downloadCancelAction ?: return
        val scope = hostScope ?: return
        cancelInFlight = true
        scope.launch {
            try {
                cancelAction()
                stopSession(keepOverlay = false)
                repository.stop()
            } finally {
                downloadCancelAction = null
                cancelInFlight = false
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

/** Mirrors Windows `utils::split_at_sentence_boundary`. */
internal fun splitAtSentenceBoundary(text: String): Pair<String, String>? {
    var lastBoundary = -1
    var i = 0
    while (i < text.length) {
        val ch = text[i]
        if (ch == '.' || ch == '?' || ch == '!') {
            val rest = text.substring(i + 1).trimStart()
            if (rest.isNotEmpty() && (rest[0].isLetter() || rest[0].isDigit())) {
                lastBoundary = i + 1
            }
        }
        i++
    }
    if (lastBoundary < 0) return null
    return Pair(text.substring(0, lastBoundary).trimEnd(), text.substring(lastBoundary).trimStart())
}

/**
 * Smooth commit threshold for non-native-punct models (ZH, RU, All8Lang).
 * Mirrors Windows `state::draft_commit_threshold_ms`.
 * Formula: 1200 / (1 + words * 0.5), CJK chars each count as one word.
 *
 * ⚠️ WARNING — THIS FUNCTION ONLY RETURNS A THRESHOLD NUMBER.
 * The caller is responsible for the FULL commit sequence when silence >= threshold:
 *   1. Append "${text}." to committedHistory
 *   2. Set streamCommittedPrefix = rawText.trimEnd()
 *   3. Reset lastDraftText and lastDraftChangeMs
 *   4. Publish with empty draft
 * Simply appending a period to the displayed draft is NOT a commit.
 * Forgetting steps 1-4 means text is never moved out of draft → never translated.
 */
internal fun draftCommitThresholdMs(draft: String): Long {
    val cjkCount = draft.count { it.code > 0x2E80 }
    val wordCount = draft.trim().split(Regex("\\s+")).count { it.isNotEmpty() } + cjkCount
    if (wordCount == 0) return Long.MAX_VALUE
    return (1200.0 / (1.0 + wordCount * 0.5)).toLong()
}

/** Returns true for sentence-ending punctuation. */
internal fun Char.isSentencePunct(): Boolean = this == '.' || this == '?' || this == '!'


internal fun computeAdaptiveTranslationIntervalMs(latencyMs: Long): Long {
    return (latencyMs + 250L)
        .coerceAtLeast(TRANSLATION_INTERVAL_MS)
        .coerceAtMost(TRANSLATION_INTERVAL_MAX_MS)
}
