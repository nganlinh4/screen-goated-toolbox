package dev.screengoated.toolbox.mobile.service.preset

import android.content.Context
import android.graphics.Bitmap
import android.os.SystemClock
import android.util.Log
import android.view.Choreographer
import android.view.WindowManager
import android.widget.Toast
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
    private val ownership = ImageCaptureOwnership()
    private var overlay: PresetImageSelectionOverlay? = null
    private var overlayOwner: Long? = null
    private var detachingOverlay = false
    private var pendingStart: Pair<Long, () -> Unit>? = null
    private var deferredFinish: Pair<Long, () -> Unit>? = null

    val isActive: Boolean get() = ownership.active || overlay != null

    fun destroy() {
        val revision = ownership.stop()
        pendingStart = null
        deferredFinish = null
        if (ownership.showInFlight) return
        if (!retireUi(revision) && ownership.isRevision(revision)) notifyCloseFailure()
    }

    fun start(
        resolvedPreset: ResolvedPreset,
        trace: ImageCaptureTrace,
        onSelectionConfirmed: (ByteArray) -> Unit,
        onColorPicked: (String) -> Unit,
        onCancelled: () -> Unit,
        onCaptureFailure: (ScreenshotCaptureFailureReason) -> Unit,
    ) {
        val owner = ownership.start()
        pendingStart = null
        deferredFinish = null
        val launch = {
            beginCapture(owner, resolvedPreset, trace, onSelectionConfirmed, onColorPicked, onCancelled, onCaptureFailure)
        }
        if (ownership.showInFlight) {
            pendingStart = owner to launch
            return
        }
        if (!retireUi(owner)) {
            val revision = ownership.retire(owner)
            if (revision != null && ownership.isRevision(revision)) {
                onCaptureFailure(ScreenshotCaptureFailureReason.REQUEST_FAILED)
            }
            return
        }
        if (!ownership.isCurrent(owner)) return
        if (ownership.canStart(owner)) launch() else pendingStart = owner to launch
    }

    private fun beginCapture(
        owner: Long,
        resolvedPreset: ResolvedPreset,
        trace: ImageCaptureTrace,
        onSelectionConfirmed: (ByteArray) -> Unit,
        onColorPicked: (String) -> Unit,
        onCancelled: () -> Unit,
        onCaptureFailure: (ScreenshotCaptureFailureReason) -> Unit,
    ) {
        if (!ownership.canStart(owner)) return
        onBubbleSuppressedChanged(true)
        if (!ownership.isCurrent(owner)) return
        onOverlaySuppressedChanged(true)
        if (!ownership.isCurrent(owner)) return
        logImageCaptureTrace(trace, "session_started")
        logImageCaptureTrace(trace, "surfaces_suppressed")
        waitForCaptureSyncFrames(owner, trace, CAPTURE_SYNC_FRAMES, resolvedPreset, onSelectionConfirmed, onColorPicked, onCancelled, onCaptureFailure)
    }

    private fun waitForCaptureSyncFrames(
        owner: Long,
        trace: ImageCaptureTrace,
        remainingFrames: Int,
        resolvedPreset: ResolvedPreset,
        onSelectionConfirmed: (ByteArray) -> Unit,
        onColorPicked: (String) -> Unit,
        onCancelled: () -> Unit,
        onCaptureFailure: (ScreenshotCaptureFailureReason) -> Unit,
    ) {
        Choreographer.getInstance().postFrameCallback {
            if (!ownership.isCurrent(owner)) return@postFrameCallback
            logImageCaptureTrace(trace, "capture_frame_synced", "remaining=$remainingFrames")
            if (remainingFrames > 1) {
                waitForCaptureSyncFrames(owner, trace, remainingFrames - 1, resolvedPreset, onSelectionConfirmed, onColorPicked, onCancelled, onCaptureFailure)
                return@postFrameCallback
            }
            val svc = SgtAccessibilityService.instance
            if (svc == null) {
                finish(owner) { onCaptureFailure(ScreenshotCaptureFailureReason.SERVICE_UNAVAILABLE) }
                return@postFrameCallback
            }
            if (!ownership.beginScreenshot(owner)) return@postFrameCallback
            logImageCaptureTrace(trace, "screenshot_requested")
            try {
                svc.captureScreenshot { result ->
                    val settled = ownership.finishScreenshot(owner)
                    if (!settled || !ownership.isCurrent(owner)) {
                        if (result is ScreenshotCaptureResult.Success && !result.bitmap.isRecycled) result.bitmap.recycle()
                        if (settled) launchPending()
                        return@captureScreenshot
                    }
                    when (result) {
                        is ScreenshotCaptureResult.Success -> {
                            logImageCaptureTrace(trace, "screenshot_success", "bitmap=${result.bitmap.width}x${result.bitmap.height}")
                            showSelectionOverlay(
                                owner,
                                resolvedPreset,
                                trace,
                                result.bitmap,
                                { bytes -> finish(owner) { onSelectionConfirmed(bytes) } },
                                { color -> finish(owner) { onColorPicked(color) } },
                                { finish(owner, retryRetainedClose = true, callback = onCancelled) },
                                onCaptureFailure,
                            )
                        }
                        is ScreenshotCaptureResult.Failure -> {
                            logImageCaptureTrace(trace, "screenshot_failure", "reason=${result.reason}")
                            finish(owner) { onCaptureFailure(result.reason) }
                        }
                    }
                }
            } catch (error: Exception) {
                if (ownership.finishScreenshot(owner)) {
                    if (ownership.isCurrent(owner)) finish(owner) { onCaptureFailure(ScreenshotCaptureFailureReason.REQUEST_FAILED) }
                    else launchPending()
                }
            }
        }
    }

    private fun showSelectionOverlay(
        owner: Long,
        resolvedPreset: ResolvedPreset,
        trace: ImageCaptureTrace,
        screenshotBitmap: Bitmap,
        onSelectionConfirmed: (ByteArray) -> Unit,
        onColorPicked: (String) -> Unit,
        onCancelled: () -> Unit,
        onCaptureFailure: (ScreenshotCaptureFailureReason) -> Unit,
    ) {
        var candidate: PresetImageSelectionOverlay? = null
        try {
            val a11yWm = SgtAccessibilityService.instance
                ?.getSystemService(Context.WINDOW_SERVICE) as? WindowManager
            candidate = PresetImageSelectionOverlay(
                context = context,
                windowManager = windowManager,
                a11yWindowManager = a11yWm,
                uiLanguage = uiLanguage(),
                title = resolvedPreset.preset.name(uiLanguage()),
                trace = trace,
                screenshotBitmap = screenshotBitmap,
                onSelectionConfirmed = onSelectionConfirmed,
                onColorPicked = onColorPicked,
                onCancelled = onCancelled,
            )
            if (!ownership.isCurrent(owner)) {
                candidate.destroy()
                return
            }
            overlay = candidate
            overlayOwner = owner
            check(ownership.beginShow(owner))
            try {
                candidate.show()
            } finally {
                ownership.finishShow(owner)
                if (!ownership.isCurrent(owner) && overlay === candidate) {
                    if (detachOverlay(candidate)) {
                        afterDetached()
                    } else notifyCloseFailure()
                }
            }
        } catch (error: Exception) {
            if ((candidate == null || overlay !== candidate) && !screenshotBitmap.isRecycled) screenshotBitmap.recycle()
            finish(owner) { onCaptureFailure(ScreenshotCaptureFailureReason.REQUEST_FAILED) }
        }
    }

    private fun finish(owner: Long, retryRetainedClose: Boolean = false, callback: () -> Unit) {
        val revision = ownership.retire(owner)
        if (revision == null) {
            if (retryRetainedClose) retryRetainedOverlay(owner)
            return
        }
        if (ownership.showInFlight) {
            deferredFinish = revision to callback
            return
        }
        if (retireUi(revision)) callback()
        else if (ownership.isRevision(revision)) notifyCloseFailure()
    }

    private fun launchPending() {
        val (owner, launch) = pendingStart ?: return
        if (!ownership.canStart(owner)) return
        pendingStart = null
        launch()
    }

    private fun retireUi(revision: Long): Boolean = retireCaptureUi(
        detach = {
            val previous = overlay
            previous == null || detachOverlay(previous)
        },
        stillCurrent = { ownership.isRevision(revision) },
        restoreBubble = { onBubbleSuppressedChanged(false) },
        restoreOverlay = { onOverlaySuppressedChanged(false) },
    )

    private fun detachOverlay(previous: PresetImageSelectionOverlay): Boolean {
        if (overlay !== previous) return true
        if (detachingOverlay || ownership.showInFlight) return false
        detachingOverlay = true
        return try {
            if (!previous.destroy()) false else {
                if (overlay === previous) {
                    overlay = null
                    overlayOwner = null
                }
                true
            }
        } finally {
            detachingOverlay = false
        }
    }

    private fun retryRetainedOverlay(owner: Long) {
        val retained = overlay ?: return
        retryRetainedCapture(
            owner = owner,
            retainedOwner = overlayOwner,
            showInFlight = ownership.showInFlight,
            detach = { detachOverlay(retained) },
            afterDetached = ::afterDetached,
            onFailure = ::notifyCloseFailure,
        )
    }

    private fun afterDetached() {
        if (pendingStart != null) {
            deferredFinish = null
            launchPending()
            return
        }
        val finish = deferredFinish
        deferredFinish = null
        val revision = finish?.first ?: ownership.currentRevision
        if (!ownership.active && ownership.isRevision(revision)) {
            if (retireUi(revision)) finish?.second?.invoke()
            else if (ownership.isRevision(revision)) notifyCloseFailure()
        }
    }

    private fun notifyCloseFailure() {
        Toast.makeText(
            context,
            overlayLocalized(
                uiLanguage(),
                "Image selection could not close. Try again.",
                "Không thể đóng vùng chọn ảnh. Hãy thử lại.",
                "이미지 선택 창을 닫을 수 없습니다. 다시 시도하세요.",
            ),
            Toast.LENGTH_LONG,
        ).show()
    }

    private companion object {
        private const val CAPTURE_SYNC_FRAMES = 2
    }
}

internal const val IMAGE_CAPTURE_LOG_TAG = "SGTImageCapture"
