package dev.screengoated.toolbox.mobile.service.preset

import android.content.Context
import android.graphics.Bitmap
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.util.Log
import android.view.Choreographer
import android.view.WindowManager
import dev.screengoated.toolbox.mobile.preset.ResolvedPreset
import dev.screengoated.toolbox.mobile.service.ScreenshotCaptureFailureReason
import dev.screengoated.toolbox.mobile.service.ScreenshotCaptureResult
import dev.screengoated.toolbox.mobile.service.SgtAccessibilityService

internal data class ImageCaptureTrace(
    val id: Long,
    val presetId: String,
    val startedAtMs: Long,
    val continuousMode: Boolean,
    val source: String,
)

internal fun logImageCaptureTrace(
    trace: ImageCaptureTrace,
    stage: String,
    extra: String = "",
) {
    val elapsedMs = SystemClock.elapsedRealtime() - trace.startedAtMs
    val suffix = extra.takeIf { it.isNotBlank() }?.let { " $it" }.orEmpty()
    Log.d(
        IMAGE_CAPTURE_LOG_TAG,
        "[trace=${trace.id}] t+${elapsedMs}ms preset=${trace.presetId} continuous=${trace.continuousMode} source=${trace.source} stage=$stage$suffix",
    )
}

internal class PresetImageCaptureSession(
    private val context: Context,
    private val windowManager: WindowManager,
    private val uiLanguage: () -> String,
    private val onBubbleSuppressedChanged: (Boolean) -> Unit,
    private val onOverlaySuppressedChanged: (Boolean) -> Unit,
) {
    private val mainHandler = Handler(Looper.getMainLooper())
    private var overlay: PresetImageSelectionOverlay? = null
    private var capturePending = false

    val isActive: Boolean
        get() = capturePending || overlay != null

    fun destroy() {
        mainHandler.removeCallbacksAndMessages(null)
        capturePending = false
        overlay?.destroy()
        overlay = null
        restoreSuppressedSurfaces()
    }

    fun start(
        resolvedPreset: ResolvedPreset,
        trace: ImageCaptureTrace,
        onSelectionConfirmed: (ByteArray) -> Unit,
        onColorPicked: (String) -> Unit,
        onCancelled: () -> Unit,
        onCaptureFailure: (ScreenshotCaptureFailureReason) -> Unit,
    ) {
        destroy()
        capturePending = true
        onBubbleSuppressedChanged(true)
        onOverlaySuppressedChanged(true)
        logImageCaptureTrace(trace, "session_started")
        logImageCaptureTrace(trace, "surfaces_suppressed")
        waitForCaptureSyncFrames(
            trace = trace,
            remainingFrames = CAPTURE_SYNC_FRAMES,
            resolvedPreset = resolvedPreset,
            onSelectionConfirmed = onSelectionConfirmed,
            onColorPicked = onColorPicked,
            onCancelled = onCancelled,
            onCaptureFailure = onCaptureFailure,
        )
    }

    private fun waitForCaptureSyncFrames(
        trace: ImageCaptureTrace,
        remainingFrames: Int,
        resolvedPreset: ResolvedPreset,
        onSelectionConfirmed: (ByteArray) -> Unit,
        onColorPicked: (String) -> Unit,
        onCancelled: () -> Unit,
        onCaptureFailure: (ScreenshotCaptureFailureReason) -> Unit,
    ) {
        Choreographer.getInstance().postFrameCallback {
            logImageCaptureTrace(trace, "capture_frame_synced", "remaining=$remainingFrames")
            if (remainingFrames > 1) {
                waitForCaptureSyncFrames(
                    trace = trace,
                    remainingFrames = remainingFrames - 1,
                    resolvedPreset = resolvedPreset,
                    onSelectionConfirmed = onSelectionConfirmed,
                    onColorPicked = onColorPicked,
                    onCancelled = onCancelled,
                    onCaptureFailure = onCaptureFailure,
                )
                return@postFrameCallback
            }

            val svc = SgtAccessibilityService.instance
            if (svc == null) {
                capturePending = false
                restoreSuppressedSurfaces()
                logImageCaptureTrace(trace, "service_unavailable")
                onCaptureFailure(ScreenshotCaptureFailureReason.SERVICE_UNAVAILABLE)
                return@postFrameCallback
            }
            logImageCaptureTrace(trace, "screenshot_requested")
            svc.captureScreenshot { result ->
                capturePending = false
                when (result) {
                    is ScreenshotCaptureResult.Success -> {
                        logImageCaptureTrace(
                            trace,
                            "screenshot_success",
                            "bitmap=${result.bitmap.width}x${result.bitmap.height}",
                        )
                        showSelectionOverlay(
                            resolvedPreset = resolvedPreset,
                            trace = trace,
                            screenshotBitmap = result.bitmap,
                            onSelectionConfirmed = {
                                destroyOverlayOnly()
                                restoreSuppressedSurfaces()
                                logImageCaptureTrace(trace, "selection_confirmed", "bytes=${it.size}")
                                onSelectionConfirmed(it)
                            },
                            onColorPicked = {
                                destroyOverlayOnly()
                                restoreSuppressedSurfaces()
                                logImageCaptureTrace(trace, "color_picked", "value=$it")
                                onColorPicked(it)
                            },
                            onCancelled = {
                                destroyOverlayOnly()
                                restoreSuppressedSurfaces()
                                logImageCaptureTrace(trace, "selection_cancelled")
                                onCancelled()
                            },
                        )
                    }

                    is ScreenshotCaptureResult.Failure -> {
                        restoreSuppressedSurfaces()
                        logImageCaptureTrace(trace, "screenshot_failure", "reason=${result.reason}")
                        onCaptureFailure(result.reason)
                    }
                }
            }
        }
    }

    private fun showSelectionOverlay(
        resolvedPreset: ResolvedPreset,
        trace: ImageCaptureTrace,
        screenshotBitmap: Bitmap,
        onSelectionConfirmed: (ByteArray) -> Unit,
        onColorPicked: (String) -> Unit,
        onCancelled: () -> Unit,
    ) {
        overlay = PresetImageSelectionOverlay(
            context = context,
            windowManager = windowManager,
            uiLanguage = uiLanguage(),
            title = resolvedPreset.preset.name(uiLanguage()),
            trace = trace,
            screenshotBitmap = screenshotBitmap,
            onSelectionConfirmed = onSelectionConfirmed,
            onColorPicked = onColorPicked,
            onCancelled = onCancelled,
        ).also { it.show() }
    }

    private fun destroyOverlayOnly() {
        overlay?.destroy()
        overlay = null
    }

    private fun restoreSuppressedSurfaces() {
        onBubbleSuppressedChanged(false)
        onOverlaySuppressedChanged(false)
    }

    private companion object {
        private const val CAPTURE_SYNC_FRAMES = 2
    }
}

internal const val IMAGE_CAPTURE_LOG_TAG = "SGTImageCapture"
