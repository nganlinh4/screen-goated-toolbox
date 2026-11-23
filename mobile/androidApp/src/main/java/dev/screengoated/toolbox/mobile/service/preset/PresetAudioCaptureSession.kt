package dev.screengoated.toolbox.mobile.service.preset

import android.content.Context
import android.graphics.Rect
import android.os.SystemClock
import android.util.Log
import android.view.WindowManager
import dev.screengoated.toolbox.mobile.AppToastBus
import dev.screengoated.toolbox.mobile.capture.AudioCaptureController
import dev.screengoated.toolbox.mobile.capture.ProjectionConsentInvalidException
import dev.screengoated.toolbox.mobile.preset.AudioApiClient
import dev.screengoated.toolbox.mobile.preset.AudioStreamingSession
import dev.screengoated.toolbox.mobile.preset.AudioStreamingTranscriptResult
import dev.screengoated.toolbox.mobile.preset.PresetModelCatalog
import dev.screengoated.toolbox.mobile.preset.PresetModelProvider
import dev.screengoated.toolbox.mobile.preset.ResolvedPreset
import dev.screengoated.toolbox.mobile.preset.initialAudioBlocks
import dev.screengoated.toolbox.mobile.preset.resolvePrompt
import dev.screengoated.toolbox.mobile.service.SgtAccessibilityService
import dev.screengoated.toolbox.mobile.shared.live.LiveSessionConfig
import dev.screengoated.toolbox.mobile.shared.live.SourceMode
import dev.screengoated.toolbox.mobile.storage.ProjectionConsentStore
import dev.screengoated.toolbox.mobile.ui.i18n.apiKeyErrorToastText
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancelChildren
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlin.math.roundToInt

internal data class PresetAudioCaptureCompletion(
    val wavBytes: ByteArray,
    val precomputedTranscript: String? = null,
    val isStreamingResult: Boolean = false,
)

private enum class PresetAudioRuntimeKind {
    STANDARD,
    GEMINI_LIVE_STREAMING,
}

internal class PresetAudioCaptureSession(
    private val context: Context,
    private val windowManager: WindowManager,
    private val projectionConsentStore: ProjectionConsentStore,
    private val audioApiClient: AudioApiClient,
    private val uiLanguage: () -> String,
    private val isDarkTheme: () -> Boolean,
    private val permissionSnapshotProvider: () -> dev.screengoated.toolbox.mobile.shared.live.PermissionSnapshot,
    private val screenBoundsProvider: () -> Rect,
    private val toastBus: AppToastBus,
) {
    private val density = context.resources.displayMetrics.density
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val audioCaptureController = AudioCaptureController(context, projectionConsentStore)
    private val htmlBuilder = PresetAudioCaptureHtmlBuilder(context)
    private var overlayWindow: PresetOverlayWindow? = null
    private var captureJob: Job? = null
    private var processingJob: Job? = null
    private var streamingSetupJob: Job? = null
    private var activePreset: ResolvedPreset? = null
    private var state: String = "idle"
    private var paused = false
    private var overlayReady = false
    private var onCancelledCallback: (() -> Unit)? = null
    private var onFailureCallback: ((PresetAudioCaptureFailure) -> Unit)? = null
    private var capturedSamples: StreamingAudioAttachment? = null
    private var hasSpoken = false
    private var firstSpeechAtMs: Long? = null
    private var lastActiveAtMs: Long = 0L
    private var processingRequested = false
    @Volatile private var runtimeKind = PresetAudioRuntimeKind.STANDARD
    @Volatile private var activeStreamingSession: AudioStreamingSession? = null
    @Volatile private var pendingStreamingChunks = PendingStreamingAudio()
    private val streamingMutex = Mutex()
    private val streamingPaste = StreamingPasteState()
    private var provisionalDelivery: ProvisionalPasteDelivery? = null
    @Volatile private var captureGeneration = 0L

    val isActive: Boolean
        get() = activePreset != null

    val activePresetId: String?
        get() = activePreset?.preset?.id

    fun toggleOrAbortIfMatching(presetId: String): Boolean {
        if (activePreset?.preset?.id != presetId) {
            return false
        }
        if (state == "processing") {
            cancelAudioCaptureAndNotify(onCancelledCallback, ::cancel)
        } else {
            stopAndSubmit()
        }
        return true
    }

    fun start(
        resolvedPreset: ResolvedPreset,
        onRecordingComplete: (PresetAudioCaptureCompletion) -> Unit,
        onCancelled: () -> Unit,
        onFailure: (PresetAudioCaptureFailure) -> Unit,
    ) {
        destroy()
        pendingStreamingChunks = PendingStreamingAudio()
        val sessionGeneration = captureGeneration

        val permissions = permissionSnapshotProvider()
        if (!permissions.recordAudioGranted) {
            onFailure(PresetAudioCaptureFailure(PresetAudioCaptureFailureReason.RECORD_PERMISSION_REQUIRED))
            return
        }
        if (resolvedPreset.preset.audioSource == "device" && !projectionConsentStore.hasConsent()) {
            onFailure(PresetAudioCaptureFailure(PresetAudioCaptureFailureReason.PROJECTION_CONSENT_REQUIRED))
            return
        }

        activePreset = resolvedPreset
        onCancelledCallback = onCancelled
        onFailureCallback = onFailure
        runtimeKind = resolveRuntimeKind(resolvedPreset)
        if (resolvedPreset.preset.autoPaste && runtimeKind != PresetAudioRuntimeKind.STANDARD) {
            val paste = ProvisionalPasteSession(
                AccessibilityProvisionalPasteTarget.capture(SgtAccessibilityService.instance),
            )
            val generation = streamingPaste.generation
            provisionalDelivery = ProvisionalPasteDelivery(scope, paste,
                capture = { AccessibilityProvisionalPasteTarget.capture(SgtAccessibilityService.instance) }) {
                if (generation == streamingPaste.generation) streamingPaste.recordInsertion(provisionalDelivery?.attempted == true)
            }
            // This session owns delivery, including refusal; the final result cannot bypass it.
            streamingPaste.accept(streamingPaste.generation)
        }
        state = if (runtimeKind == PresetAudioRuntimeKind.GEMINI_LIVE_STREAMING) {
            "initializing"
        } else {
            "warmup"
        }
        paused = false
        overlayReady = false
        processingRequested = false
        val capturedSamples = try {
            StreamingAudioAttachment(context.cacheDir).also { this.capturedSamples = it }
        } catch (error: Exception) {
            Log.e(TAG, "Unable to create audio capture storage", error)
            onFailure(PresetAudioCaptureFailure(PresetAudioCaptureFailureReason.CAPTURE_FAILED, error.message.orEmpty()))
            destroy()
            return
        }
        pendingStreamingChunks.clear()
        hasSpoken = false
        firstSpeechAtMs = null
        lastActiveAtMs = SystemClock.elapsedRealtime()
        if (!resolvedPreset.preset.hideRecordingUi) {
            showOverlay()
        }
        updateOverlay(rms = 0f)
        startStreamingSessionIfNeeded(resolvedPreset)

        captureJob = scope.launch {
            try {
                val sourceMode = if (resolvedPreset.preset.audioSource == "device") {
                    SourceMode.DEVICE
                } else {
                    SourceMode.MIC
                }
                withContext(Dispatchers.IO) {
                    audioCaptureController.open(
                        config = LiveSessionConfig(sourceMode = sourceMode),
                        onRms = { rms ->
                            scope.launch {
                                if (captureGeneration == sessionGeneration) handleRms(rms, resolvedPreset)
                            }
                        },
                    ).collect { chunk ->
                        if (captureGeneration != sessionGeneration || paused || processingRequested) {
                            return@collect
                        }
                        when (capturedSamples.append(chunk, runtimeKind != PresetAudioRuntimeKind.STANDARD)) {
                            AudioAttachmentAppend.OMITTED -> Log.i(TAG, "audio attachment omitted after 600 seconds; transcription continues")
                            AudioAttachmentAppend.LIMIT_REACHED -> {
                                withContext(Dispatchers.Main.immediate) {
                                    if (captureGeneration == sessionGeneration) {
                                        toastBus.show(when (uiLanguage()) {
                                            "vi" -> "Đã dừng ghi âm tại giới hạn 10 phút."
                                            "ko" -> "10분 제한에 도달하여 녹음을 중지했습니다."
                                            else -> "Recording stopped at the 10-minute limit."
                                        })
                                        stopAndSubmit()
                                    }
                                }
                                return@collect
                            }
                            AudioAttachmentAppend.IGNORED -> return@collect
                            else -> Unit
                        }
                        try {
                            appendStreamingChunk(chunk, sessionGeneration)
                        } catch (cancelled: CancellationException) {
                            throw cancelled
                        } catch (error: Exception) {
                            withContext(Dispatchers.Main.immediate) {
                                if (captureGeneration == sessionGeneration) degradeStreaming(error)
                            }
                        }
                    }
                }
            } catch (_: CancellationException) {
                // expected on stop/cancel
            } catch (error: SecurityException) {
                if (captureGeneration != sessionGeneration) return@launch
                Log.e(TAG, "audio capture security failure: preset=${resolvedPreset.preset.id}", error)
                onFailure(PresetAudioCaptureFailure(
                    reason = PresetAudioCaptureFailureReason.CAPTURE_FAILED,
                    detail = "${error::class.java.simpleName}:${error.message}",
                ))
                destroy()
            } catch (error: ProjectionConsentInvalidException) {
                if (captureGeneration != sessionGeneration) return@launch
                projectionConsentStore.clear()
                onFailure(PresetAudioCaptureFailure(
                    reason = PresetAudioCaptureFailureReason.PROJECTION_CONSENT_REQUIRED,
                    detail = "${error::class.java.simpleName}:${error.message}",
                ))
                destroy()
            } catch (error: Throwable) {
                if (captureGeneration != sessionGeneration) return@launch
                Log.e(TAG, "audio capture fatal failure: preset=${resolvedPreset.preset.id}", error)
                onFailure(PresetAudioCaptureFailure(
                    reason = PresetAudioCaptureFailureReason.CAPTURE_FAILED,
                    detail = "${error::class.java.simpleName}:${error.message}",
                ))
                destroy()
            }
        }

        processingJob = scope.launch {
            captureJob?.join()
            if (captureGeneration != sessionGeneration || !processingRequested) {
                return@launch
            }
            try {
                val wavBytes = withContext(Dispatchers.Default) {
                    capturedSamples.toWavBytes()
                }
                val streamingTranscript = finalizeStreamingOrFallback(
                    finalize = { finalizeStreamingTranscript() },
                    onFailure = { Log.w(TAG, "Streaming finalization failed; preserving WAV fallback", it) },
                )
                provisionalDelivery?.finish()
                streamingPaste.invalidate()
                if (capturedSamples.omitted && streamingTranscript?.transcript.isNullOrBlank()) {
                    error("Streaming transcription unavailable and session exceeded the audio attachment limit")
                }
                if (wavBytes.size <= 44 && streamingTranscript?.transcript.isNullOrBlank()) {
                    onCancelled()
                } else {
                    onRecordingComplete(
                        PresetAudioCaptureCompletion(
                            wavBytes = wavBytes,
                            precomputedTranscript = streamingTranscript?.transcript?.takeIf { it.isNotBlank() },
                            isStreamingResult = streamingPaste.suppressFinalPaste || streamingTranscript?.producedRealtimePaste == true,
                        ),
                    )
                }
            } catch (cancelled: CancellationException) {
                throw cancelled
            } catch (error: Exception) {
                if (captureGeneration == sessionGeneration) {
                    Log.e(TAG, "Unable to finish audio capture", error)
                    onFailure(PresetAudioCaptureFailure(PresetAudioCaptureFailureReason.CAPTURE_FAILED, error.message.orEmpty()))
                }
            } finally {
                if (captureGeneration == sessionGeneration) destroy()
            }
        }
    }

    fun stopAndSubmit() {
        if (!isActive || processingRequested) {
            return
        }
        processingRequested = true
        provisionalDelivery?.beginDrain()
        state = "processing"
        updateOverlay(rms = 0f)
        captureJob?.cancel()
    }

    fun togglePause() {
        if (!isActive || processingRequested) {
            return
        }
        paused = !paused
        state = if (paused) "paused" else activeCaptureState()
        updateOverlay(rms = 0f)
    }

    fun refreshOverlayForPreferences() {
        val preset = activePreset ?: return
        if (preset.preset.hideRecordingUi) {
            return
        }
        val window = overlayWindow ?: return
        overlayReady = false
        window.loadHtmlContent(buildOverlayHtml())
    }

    fun handleMessage(message: String) {
        when {
            message == "cancel" -> {
                cancelAudioCaptureAndNotify(onCancelledCallback, ::cancel)
            }

            message == "pause_toggle" -> togglePause()
            message == "ready" -> {
                overlayReady = true
                overlayWindow?.runScript("document.body.classList.add('visible');")
                updateOverlay(rms = 0f)
            }

            message == "drag_window" -> Unit
            message.startsWith("{") -> {
                val payload = message.jsonOrNull() ?: return
                if (payload.optString("type") == "dragAudioWindow") {
                    overlayWindow?.moveBy(
                        dx = payload.optDouble("dx", 0.0).roundToInt(),
                        dy = payload.optDouble("dy", 0.0).roundToInt(),
                        screenBounds = screenBoundsProvider(),
                    )
                }
            }
        }
    }

    fun cancel() {
        processingRequested = false
        captureJob?.cancel()
        processingJob?.cancel()
        destroy()
    }

    fun destroy() {
        captureGeneration++
        provisionalDelivery?.cancel()
        provisionalDelivery = null
        streamingPaste.reset()
        scope.coroutineContext.cancelChildren()
        captureJob = null
        processingJob = null
        streamingSetupJob = null
        activeStreamingSession?.cancel()
        activeStreamingSession = null
        pendingStreamingChunks.clear()
        overlayWindow?.destroy()
        overlayWindow = null
        activePreset = null
        onCancelledCallback = null
        onFailureCallback = null
        state = "idle"
        paused = false
        overlayReady = false
        processingRequested = false
        runtimeKind = PresetAudioRuntimeKind.STANDARD
        capturedSamples?.clear()
        capturedSamples = null
    }

    private fun showOverlay() {
        val screen = screenBoundsProvider()
        val (width, height) = audioRecordingWindowDimensions(
            screenWidth = screen.width(),
            screenHeight = screen.height(),
            density = density,
        )
        val spec = PresetOverlayWindowSpec(
            width = width,
            height = height,
            x = screen.centerX() - width / 2,
            y = (screen.centerY() - height / 2).coerceAtLeast(dp(48)),
            focusable = false,
            htmlContent = buildOverlayHtml(width = width, height = height),
            clipToOutline = false,
        )
        overlayWindow = PresetOverlayWindow(
            context = context,
            windowManager = windowManager,
            spec = spec,
            onMessage = ::handleMessage,
        ).also { window ->
            window.setOnPageFinishedListener {
                window.runScript("window.resetState && window.resetState();")
            }
            window.show()
        }
    }

    private fun buildOverlayHtml(
        width: Int = overlayWindow?.currentBounds()?.width ?: audioRecordingWindowDimensions(
            screenWidth = screenBoundsProvider().width(),
            screenHeight = screenBoundsProvider().height(),
            density = density,
        ).first,
        height: Int = overlayWindow?.currentBounds()?.height ?: audioRecordingWindowDimensions(
            screenWidth = screenBoundsProvider().width(),
            screenHeight = screenBoundsProvider().height(),
            density = density,
        ).second,
    ): String {
        return htmlBuilder.build(
            PresetAudioCaptureHtmlSettings(
                uiLanguage = uiLanguage(),
                isDark = isDarkTheme(),
                windowWidth = width,
                windowHeight = height,
            ),
        )
    }

    private fun startStreamingSessionIfNeeded(resolvedPreset: ResolvedPreset) {
        if (runtimeKind == PresetAudioRuntimeKind.STANDARD) {
            return
        }
        val audioBlock = resolvedPreset.preset.initialAudioBlocks().firstOrNull() ?: return
        val sessionGeneration = captureGeneration
        val delivery = provisionalDelivery
        streamingSetupJob = scope.launch {
            try {
                acquireStreamingSession(
                    acquire = {
                        audioApiClient.openStreamingSession(
                            modelId = audioBlock.model,
                            _prompt = audioBlock.resolvePrompt(),
                            apiKeys = apiKeys(),
                            uiLanguage = uiLanguage(),
                            onChunk = { chunk -> delivery?.finalSegment(chunk) },
                            onInterim = { chunk -> delivery?.interim(chunk) },
                        )
                    },
                    install = { session -> streamingMutex.withLock {
                        check(captureGeneration == sessionGeneration && runtimeKind != PresetAudioRuntimeKind.STANDARD)
                        activeStreamingSession = requireNotNull(session) { "Streaming session unavailable" }
                        flushPendingStreamingChunks(session)
                    } },
                )
            } catch (cancelled: CancellationException) {
                throw cancelled
            } catch (error: Throwable) {
                apiKeyErrorToastText(error.message ?: error.toString(), uiLanguage())?.let(toastBus::show)
                if (captureGeneration == sessionGeneration) degradeStreaming(error)
            } finally {
                if (captureGeneration == sessionGeneration && state == "initializing" && !processingRequested) {
                    state = if (paused) "paused" else "warmup"
                    updateOverlay(rms = 0f)
                }
            }
        }
    }

    private fun degradeStreaming(error: Throwable) {
        Log.w(TAG, "Streaming transcription failed", error)
        if (capturedSamples?.fallBackToFinite() != true) {
            onFailureCallback?.invoke(PresetAudioCaptureFailure(PresetAudioCaptureFailureReason.CAPTURE_FAILED, error.message))
            destroy()
            return
        }
        provisionalDelivery?.cancel()
        streamingPaste.invalidate()
        runtimeKind = PresetAudioRuntimeKind.STANDARD
        streamingSetupJob?.cancel()
        streamingSetupJob = null
        activeStreamingSession?.cancel()
        activeStreamingSession = null
        pendingStreamingChunks.clear()
        if (state == "initializing" && !processingRequested) {
            state = if (paused) "paused" else "warmup"
            updateOverlay(rms = 0f)
        }
    }

    private suspend fun appendStreamingChunk(chunk: ShortArray, generation: Long) = streamingMutex.withLock {
        val pending = pendingStreamingChunks
        val session = activeStreamingSession
        if (captureGeneration != generation) return@withLock
        if (session != null) {
            session.appendPcm16Chunk(chunk)
            if (state == "initializing") {
                state = "warmup"
            }
            return@withLock
        }
        if (runtimeKind == PresetAudioRuntimeKind.STANDARD || streamingSetupJob == null) {
            return@withLock
        }
        pending.append(chunk)
    }

    private suspend fun finalizeStreamingTranscript(): AudioStreamingTranscriptResult? {
        if (runtimeKind == PresetAudioRuntimeKind.STANDARD) {
            return null
        }
        runCatching { streamingSetupJob?.join() }
        val session = activeStreamingSession ?: return null
        return try {
            flushPendingStreamingChunks(session)
            session.finish().copy(producedRealtimePaste = streamingPaste.inserted)
        } finally {
            session.cancel()
            activeStreamingSession = null
        }
    }

    private suspend fun flushPendingStreamingChunks(session: AudioStreamingSession?) {
        val streamingSession = session ?: return
        while (true) streamingSession.appendPcm16Chunk(pendingStreamingChunks.removeFirstOrNull() ?: break)
    }

    private fun handleRms(rms: Float, resolvedPreset: ResolvedPreset) {
        if (processingRequested) {
            return
        }
        if (!paused) {
            val now = SystemClock.elapsedRealtime()
            if (dev.screengoated.toolbox.mobile.shared.live.SpeechActivity.autoStopActivity(rms)) {
                if (!hasSpoken) {
                    firstSpeechAtMs = now
                }
                hasSpoken = true
                lastActiveAtMs = now
            } else if (resolvedPreset.preset.autoStopRecording && hasSpoken) {
                val recordingDuration = now - (firstSpeechAtMs ?: now)
                if (recordingDuration >= MIN_RECORDING_MS && now - lastActiveAtMs > SILENCE_LIMIT_MS) {
                    stopAndSubmit()
                    return
                }
            }
        }
        if (state == "warmup" && rms >= WARMUP_THRESHOLD) {
            state = "recording"
        }
        if (state == "recording" || state == "warmup" || state == "paused" || state == "initializing") {
            updateOverlay(rms)
        }
    }

    private fun updateOverlay(rms: Float) {
        val window = overlayWindow ?: return
        val script = "window.updateState && window.updateState(${jsonQuote(state)}, ${rms.coerceIn(0f, 1f)});"
        if (overlayReady) {
            window.runScript(script)
        } else {
            window.runScript(script)
        }
    }

    private fun resolveRuntimeKind(resolvedPreset: ResolvedPreset): PresetAudioRuntimeKind {
        val modelId = resolvedPreset.preset.initialAudioBlocks().firstOrNull()?.model.orEmpty()
        return when (PresetModelCatalog.getById(modelId)?.provider) {
            PresetModelProvider.GEMINI_LIVE -> PresetAudioRuntimeKind.GEMINI_LIVE_STREAMING
            else -> PresetAudioRuntimeKind.STANDARD
        }
    }

    private fun activeCaptureState(): String {
        return if (runtimeKind == PresetAudioRuntimeKind.GEMINI_LIVE_STREAMING && activeStreamingSession == null) {
            "initializing"
        } else {
            "recording"
        }
    }

    private fun apiKeys(): dev.screengoated.toolbox.mobile.preset.ApiKeys {
        val appContainer = (context.applicationContext as dev.screengoated.toolbox.mobile.SgtMobileApplication).appContainer
        return dev.screengoated.toolbox.mobile.preset.ApiKeys(
            geminiKey = appContainer.repository.currentApiKey(),
            groqKey = appContainer.repository.currentGroqApiKey(),
            openRouterKey = appContainer.repository.currentOpenRouterApiKey(),
            nvidiaKey = appContainer.repository.currentNvidiaApiKey(),
            ollamaBaseUrl = appContainer.repository.currentOllamaUrl(),
        )
    }

    private fun dp(value: Int): Int = (value * density).roundToInt()

    private fun jsonQuote(value: String): String = org.json.JSONObject.quote(value)

    private companion object {
        private const val TAG = "PresetAudioCapture"
        const val WARMUP_THRESHOLD = 0.001f
        const val SILENCE_LIMIT_MS = 800L
        const val MIN_RECORDING_MS = 2_000L
    }
}
