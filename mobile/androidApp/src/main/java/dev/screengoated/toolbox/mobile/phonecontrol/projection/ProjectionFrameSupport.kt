package dev.screengoated.toolbox.mobile.phonecontrol.projection

import android.content.Context
import android.graphics.Bitmap
import android.hardware.display.DisplayManager
import android.hardware.display.VirtualDisplay
import android.media.Image
import android.media.ImageReader
import android.os.Build
import android.util.DisplayMetrics
import android.view.Display
import android.view.Surface
import android.view.WindowManager
import kotlinx.coroutines.CompletableDeferred

internal class RetainedProjectionImage(
    val image: Image,
    val capturedAtMs: Long,
)

internal enum class ProjectionFrameDisposition { DROP, DECODE, RETAIN }

/**
 * A frame is decoded when a capture waits for it or nothing is cached yet.
 * Otherwise it is retained undecoded as the newest frame, never dropped, so a
 * capture on a static screen gets the last screen content, not a stale one.
 */
internal fun projectionFrameDisposition(
    active: Boolean,
    captureWaiting: Boolean,
    cacheEmpty: Boolean,
): ProjectionFrameDisposition = when {
    !active -> ProjectionFrameDisposition.DROP
    captureWaiting || cacheEmpty -> ProjectionFrameDisposition.DECODE
    else -> ProjectionFrameDisposition.RETAIN
}

/**
 * Below API 34 a rotation is not reported as a captured-content resize, so the
 * virtual display follows the default display's size itself. On API 34+ the
 * platform callback owns resizing.
 */
internal fun shouldFollowDisplayResize(
    sdkInt: Int,
    currentWidth: Int,
    currentHeight: Int,
    displayWidth: Int,
    displayHeight: Int,
): Boolean = sdkInt < Build.VERSION_CODES.UPSIDE_DOWN_CAKE &&
    displayWidth > 0 && displayHeight > 0 &&
    (displayWidth != currentWidth || displayHeight != currentHeight)

internal data class RetiredProjectionResources(
    val display: VirtualDisplay?,
    val reader: ImageReader?,
    val frame: CachedProjectionFrame?,
)

internal data class PublishedProjectionFrame(
    val request: CompletableDeferred<PhoneControlProjectionFrameResult>?,
    val callerBitmap: Bitmap?,
)

internal data class ProjectionImageDiagnostics(
    val width: Int,
    val height: Int,
    val pixelStride: Int,
    val rowStride: Int,
    val bufferBytes: Int,
)

internal fun shouldSummarizeProjectionDecodeFailure(consecutiveFailures: Int): Boolean =
    consecutiveFailures == 1 ||
        consecutiveFailures % PROJECTION_DECODE_FAILURE_SUMMARY_INTERVAL == 0

internal fun projectionImageDiagnostics(image: Image): ProjectionImageDiagnostics {
    val plane = runCatching { image.planes.firstOrNull() }.getOrNull()
    return ProjectionImageDiagnostics(
        width = runCatching { image.width }.getOrDefault(0),
        height = runCatching { image.height }.getOrDefault(0),
        pixelStride = runCatching { plane?.pixelStride ?: 0 }.getOrDefault(0),
        rowStride = runCatching { plane?.rowStride ?: 0 }.getOrDefault(0),
        bufferBytes = runCatching { plane?.buffer?.remaining() ?: 0 }.getOrDefault(0),
    )
}

private const val PROJECTION_DECODE_FAILURE_SUMMARY_INTERVAL = 300

internal data class CachedProjectionFrame(
    val bitmap: Bitmap,
    val capturedAtMs: Long,
    val metadata: ProjectionDisplayMetadata,
) {
    fun copyForCaller(): PhoneControlProjectionFrameResult.Success =
        PhoneControlProjectionFrameResult.Success(
            bitmap.copy(Bitmap.Config.ARGB_8888, true),
            capturedAtMs,
            metadata.rotation,
            metadata.densityDpi,
        )
}

internal data class ProjectionDisplayMetadata(
    val rotation: Int,
    val densityDpi: Int,
)

internal data class ProjectionDimensions(
    val width: Int,
    val height: Int,
    val densityDpi: Int,
)

internal fun projectionDimensions(context: Context): ProjectionDimensions {
    val windowManager = context.getSystemService(WindowManager::class.java)
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
        val bounds = windowManager.maximumWindowMetrics.bounds
        return ProjectionDimensions(
            width = bounds.width().coerceAtLeast(1),
            height = bounds.height().coerceAtLeast(1),
            densityDpi = context.resources.configuration.densityDpi,
        )
    }
    @Suppress("DEPRECATION")
    val metrics = DisplayMetrics().also(windowManager.defaultDisplay::getRealMetrics)
    return ProjectionDimensions(
        width = metrics.widthPixels.coerceAtLeast(1),
        height = metrics.heightPixels.coerceAtLeast(1),
        densityDpi = metrics.densityDpi,
    )
}

internal fun projectionDisplayMetadata(context: Context): ProjectionDisplayMetadata {
    val displayManager = context.getSystemService(DisplayManager::class.java)
    val windowManager = context.getSystemService(WindowManager::class.java)
    @Suppress("DEPRECATION")
    val rotation = displayManager
        ?.getDisplay(Display.DEFAULT_DISPLAY)
        ?.rotation
        ?: runCatching { windowManager.defaultDisplay.rotation }
            .getOrDefault(Surface.ROTATION_0)
    return ProjectionDisplayMetadata(
        rotation = rotation,
        densityDpi = context.resources.configuration.densityDpi.coerceAtLeast(1),
    )
}

internal fun Image.toBitmap(): Bitmap {
    val plane = planes.firstOrNull() ?: error("Projection image has no pixel plane")
    val visiblePixels = copyVisibleRgbaBytes(
        source = plane.buffer,
        width = width,
        height = height,
        pixelStride = plane.pixelStride,
        rowStride = plane.rowStride,
    )
    return Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888).also {
        it.copyPixelsFromBuffer(visiblePixels)
    }
}
