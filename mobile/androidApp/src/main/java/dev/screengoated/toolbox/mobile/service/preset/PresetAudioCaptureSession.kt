package dev.screengoated.toolbox.mobile.service.preset

import android.content.Context
import android.graphics.Rect
import android.os.SystemClock
import android.util.Log
import android.view.WindowManager
import dev.screengoated.toolbox.mobile.capture.AudioCaptureController
import dev.screengoated.toolbox.mobile.capture.ProjectionConsentInvalidException
import dev.screengoated.toolbox.mobile.preset.AudioApiClient
import dev.screengoated.toolbox.mobile.preset.AudioStreamingSession
import dev.screengoated.toolbox.mobile.preset.AudioStreamingTranscriptResult
import dev.screengoated.toolbox.mobile.preset.PresetAudioCodec
import dev.screengoated.toolbox.mobile.preset.PresetModelCatalog
import dev.screengoated.toolbox.mobile.preset.PresetModelProvider
import dev.screengoated.toolbox.mobile.preset.ResolvedPreset
import dev.screengoated.toolbox.mobile.preset.resolvePrompt
import dev.screengoated.toolbox.mobile.shared.live.LiveSessionConfig
import dev.screengoated.toolbox.mobile.shared.live.SourceMode
import dev.screengoated.toolbox.mobile.shared.preset.BlockType
import dev.screengoated.toolbox.mobile.storage.ProjectionConsentStore
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancelChildren
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.launch
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
    PARAKEET_STREAMING,
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
    private val onStreamingTextChunk: (String) -> Boolean = { false },
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
    private val capturedSamples = ArrayList<Short>(16_000 * 30)
    private var hasSpoken = false
    private var firstSpeechAtMs: Long? = null
    private var lastActiveAtMs: Long = 0L
    private var processingRequested = false
    private var runtimeKind = PresetAudioRuntimeKind.STANDARD
    private var activeStreamingSession: AudioStreamingSession? = null
    private val pendingStreamingChunks = ArrayDeque<ShortArray>()
    private var producedRealtimePaste = false

    val isActive: Boolean
        get() = activePreset != null

    val activePresetId: String?
        get() = activePreset?.preset?.id

    fun toggleOrAbortIfMatching(presetId: String): Boolean {
        if (activePreset?.preset?.id != presetId) {
            return false
        }
        if (state == "processing") {
            cancel()
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

        val permissions = permissionSnapshotProvider()
        if (!permissions.recordAudioGranted) {
            onFailure(PresetAudioCaptureFailure(PresetAudioCaptureFailureReason.RECORD_PERMISSION_REQUIRED))
            return
        }
        if (resolvedPreset.preset.audioSource == "device" && !permissions.mediaProjectionGranted) {
            onFailure(PresetAudioCaptureFailure(PresetAudioCaptureFailureReason.PROJECTION_CONSENT_REQUIRED))
            return
        }

        activePreset = resolvedPreset
        onCancelledCallback = onCancelled
        runtimeKind = resolveRuntimeKind(resolvedPreset)
        state = if (runtimeKind == PresetAudioRuntimeKind.GEMINI_LIVE_STREAMING) {
            "initializing"
        } else {
            "warmup"
        }
        paused = false
        overlayReady = false
        processingRequested = false
        capturedSamples.clear()
        pendingStreamingChunks.clear()
        hasSpoken = false
        firstSpeechAtMs = null
        lastActiveAtMs = SystemClock.elapsedRealtime()
        producedRealtimePaste = false
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
                            scope.launch { handleRms(rms, resolvedPreset) }
                        },
                    ).collect { chunk ->
                        if (paused || processingRequested) {
                            return@collect
                        }
                        chunk.forEach(capturedSamples::add)
                        runCatching { appendStreamingChunk(chunk) }
                            .onFailure { error ->
                                Log.w(TAG, "Streaming session degraded to standard mode", error)
                                activeStreamingSession?.cancel()
                                activeStreamingSession = null
                                pendingStreamingChunks.clear()
                                runtimeKind = PresetAudioRuntimeKind.STANDARD
                                if (state == "initializing") {
                                    state = "warmup"
                                    updateOverlay(rms = 0f)
                                }
                            }
                    }
                }
            } catch (_: CancellationException) {
                // expected on stop/cancel
            } catch (error: SecurityException) {
                Log.e(TAG, "audio capture security failure: preset=${resolvedPreset.preset.id}", error)
                onFailure(
                    PresetAudioCaptureFailure(
                        reason = PresetAudioCaptureFailureReason.CAPTURE_FAILED,
                        detail = "${error::class.java.simpleName}:${error.message}",
                    ),
                )
                destroy()
            } catch (error: ProjectionConsentInvalidException) {
                projectionConsentStore.clear()
                onFailure(
                    PresetAudioCaptureFailure(
                        reason = PresetAudioCaptureFailureReason.PROJECTION_CONSENT_REQUIRED,
                        detail = "${error::class.java.simpleName}:${error.message}",
                    ),
                )
                destroy()
            } catch (error: Throwable) {
                Log.e(TAG, "audio capture fatal failure: preset=${resolvedPreset.preset.id}", error)
                onFailure(
                    PresetAudioCaptureFailure(
                        reason = PresetAudioCaptureFailureReason.CAPTURE_FAILED,
                        detail = "${error::class.java.simpleName}:${error.message}",
                    ),
                )
                destroy()
            }
        }

        processingJob = scope.launch {
            captureJob?.join()
            if (!processingRequested) {
                return@launch
            }
            val wavBytes = withContext(Dispatchers.Default) {
                PresetAudioCodec.encodePcm16MonoWav(capturedSamples.toShortArray())
            }
            val streamingTranscript = finalizeStreamingTranscript()
            if (wavBytes.size <= 44) {
                onCancelled()
            } else {
                onRecordingComplete(
                    PresetAudioCaptureCompletion(
                        wavBytes = wavBytes,
                        precomputedTranscript = streamingTranscript?.transcript?.takeIf { it.isNotBlank() },
                        isStreamingResult = streamingTranscript?.producedRealtimePaste == true,
                    ),
                )
            }
            destroy()
        }
    }

    fun stopAndSubmit() {
        if (!isActive || processingRequested) {
            return
        }
        processingRequested = true
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
                cancel()
                onCancelledCallback?.invoke()
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
        state = "idle"
        paused = false
        overlayReady = false
        processingRequested = false
        runtimeKind = PresetAudioRuntimeKind.STANDARD
        capturedSamples.clear()
        producedRealtimePaste = false
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
        val audioBlock = resolvedPreset.preset.blocks.firstOrNull { it.blockType == BlockType.AUDIO } ?: return
        streamingSetupJob = scope.launch {
            try {
                val session = withContext(Dispatchers.IO) {
                    audioApiClient.openStreamingSession(
                        modelId = audioBlock.model,
                        _prompt = audioBlock.resolvePrompt(),
                        apiKeys = apiKeys(),
                        uiLanguage = uiLanguage(),
                        onChunk = { chunk ->
                            scope.launch { handleStreamingTranscriptChunk(chunk) }
                        },
                    )
                }
                activeStreamingSession = session
                flushPendingStreamingChunks(session)
            } catch (cancelled: CancellationException) {
                throw cancelled
            } catch (ignored: Throwable) {
                activeStreamingSession = null
            } finally {
                if (state == "initializing" && !processingRequested) {
                    state = if (paused) "paused" else "warmup"
                    updateOverlay(rms = 0f)
                }
            }
        }
    }

    private suspend fun appendStreamingChunk(chunk: ShortArray) {
        val session = activeStreamingSession
        if (session != null) {
            session.appendPcm16Chunk(chunk)
            if (state == "initializing") {
                state = "warmup"
            }
            return
        }
        if (runtimeKind == PresetAudioRuntimeKind.STANDARD || streamingSetupJob == null) {
            return
        }
        pendingStreamingChunks += chunk.copyOf()
    }

    private suspend fun finalizeStreamingTranscript(): AudioStreamingTranscriptResult? {
        if (runtimeKind == PresetAudioRuntimeKind.STANDARD) {
            return null
        }
        runCatching { streamingSetupJob?.join() }
        val session = activeStreamingSession ?: return null
        return try {
            flushPendingStreamingChunks(session)
            session.finish().copy(producedRealtimePaste = producedRealtimePaste)
        } finally {
            activeStreamingSession = null
        }
    }

    private suspend fun flushPendingStreamingChunks(session: AudioStreamingSession?) {
        val streamingSession = session ?: return
        while (pendingStreamingChunks.isNotEmpty()) {
            streamingSession.appendPcm16Chunk(pendingStreamingChunks.removeFirst())
        }
    }

    private fun handleRms(rms: Float, resolvedPreset: ResolvedPreset) {
        if (processingRequested) {
            return
        }
        if (!paused) {
            val now = SystemClock.elapsedRealtime()
            if (rms > NOISE_THRESHOLD) {
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

    private fun handleStreamingTranscriptChunk(chunk: String) {
        if (chunk.isBlank()) {
            return
        }
        val preset = activePreset?.preset ?: return
        if (!preset.autoPaste) {
            return
        }
        producedRealtimePaste = onStreamingTextChunk(chunk) || producedRealtimePaste
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
        val modelId = resolvedPreset.preset.blocks.firstOrNull { it.blockType == BlockType.AUDIO }?.model.orEmpty()
        return when (PresetModelCatalog.getById(modelId)?.provider) {
            PresetModelProvider.GEMINI_LIVE -> PresetAudioRuntimeKind.GEMINI_LIVE_STREAMING
            PresetModelProvider.PARAKEET -> PresetAudioRuntimeKind.PARAKEET_STREAMING
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
            cerebrasKey = appContainer.repository.currentCerebrasApiKey(),
            groqKey = appContainer.repository.currentGroqApiKey(),
            openRouterKey = appContainer.repository.currentOpenRouterApiKey(),
            ollamaBaseUrl = appContainer.repository.currentOllamaUrl(),
        )
    }

    private fun dp(value: Int): Int = (value * density).roundToInt()

    private fun jsonQuote(value: String): String = org.json.JSONObject.quote(value)

    private companion object {
        private const val TAG = "PresetAudioCapture"
        const val WARMUP_THRESHOLD = 0.001f
        const val NOISE_THRESHOLD = 0.015f
        const val SILENCE_LIMIT_MS = 800L
        const val MIN_RECORDING_MS = 2_000L
    }
}
