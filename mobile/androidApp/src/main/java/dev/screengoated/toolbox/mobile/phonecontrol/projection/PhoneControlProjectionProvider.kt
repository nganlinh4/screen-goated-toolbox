package dev.screengoated.toolbox.mobile.phonecontrol.projection

import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.PixelFormat
import android.hardware.display.DisplayManager
import android.hardware.display.VirtualDisplay
import android.media.Image
import android.media.ImageReader
import android.media.projection.MediaProjection
import android.media.projection.MediaProjectionManager
import android.os.Build
import android.os.Handler
import android.os.HandlerThread
import android.os.SystemClock
import android.util.DisplayMetrics
import android.view.Display
import android.view.Surface
import android.view.WindowManager
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlLog as Log
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.withTimeoutOrNull
import java.util.concurrent.atomic.AtomicBoolean

internal sealed interface PhoneControlProjectionStartResult {
    data class Ready(
        val width: Int,
        val height: Int,
        val densityDpi: Int,
    ) : PhoneControlProjectionStartResult

    data class Failure(val code: String) : PhoneControlProjectionStartResult
}

internal sealed interface PhoneControlProjectionFrameResult {
    data class Success(
        val bitmap: Bitmap,
        val capturedAtMs: Long,
        val rotation: Int,
        val densityDpi: Int,
    ) : PhoneControlProjectionFrameResult

    data class Failure(
        val code: String,
        val retryable: Boolean,
    ) : PhoneControlProjectionFrameResult
}

internal object PhoneControlProjectionProvider {
    private val lock = Any()

    @Volatile
    private var session: ProjectionSession? = null

    val isReady: Boolean
        get() = session?.isReady == true

    fun start(
        context: Context,
        grant: PhoneControlProjectionGrant,
        onProjectionStopped: () -> Unit,
    ): PhoneControlProjectionStartResult {
        stop()
        val manager = context.getSystemService(MediaProjectionManager::class.java)
            ?: return PhoneControlProjectionStartResult.Failure("projection_manager_unavailable")
        val projection = try {
            manager.getMediaProjection(grant.resultCode, Intent(grant.data))
        } catch (error: Throwable) {
            Log.e(TAG, "projection_start_failed code=projection_grant_invalid", error)
            null
        } ?: return PhoneControlProjectionStartResult.Failure("projection_grant_invalid")
        val dimensions = try {
            projectionDimensions(context)
        } catch (error: Throwable) {
            Log.e(TAG, "projection_start_failed code=projection_display_unavailable", error)
            runCatching { projection.stop() }
            return PhoneControlProjectionStartResult.Failure("projection_display_unavailable")
        }
        val candidate = ProjectionSession(
            projection = projection,
            initialDimensions = dimensions,
            displayMetadata = { projectionDisplayMetadata(context) },
            onProjectionStopped = onProjectionStopped,
        )
        return try {
            candidate.start()
            check(candidate.isReady) { "Projection stopped during startup." }
            synchronized(lock) {
                check(candidate.isReady) { "Projection stopped during startup." }
                session = candidate
            }
            Log.i(
                TAG,
                "projection_session_started width=${dimensions.width} " +
                    "height=${dimensions.height} density_dpi=${dimensions.densityDpi}",
            )
            PhoneControlProjectionStartResult.Ready(
                dimensions.width,
                dimensions.height,
                dimensions.densityDpi,
            )
        } catch (error: Throwable) {
            Log.e(TAG, "projection_start_failed code=projection_virtual_display_failed", error)
            candidate.close(requested = true)
            PhoneControlProjectionStartResult.Failure("projection_virtual_display_failed")
        }
    }

    suspend fun capture(): PhoneControlProjectionFrameResult {
        val active = session
            ?: return PhoneControlProjectionFrameResult.Failure(
                code = "projection_unavailable",
                retryable = false,
            )
        return active.capture()
    }

    fun stop() {
        val retiring = synchronized(lock) {
            val active = session
            session = null
            active
        }
        retiring?.close(requested = true)
    }

    private const val TAG = "SGTPhoneControlProjection"
}

private class ProjectionSession(
    private val projection: MediaProjection,
    initialDimensions: ProjectionDimensions,
    private val displayMetadata: () -> ProjectionDisplayMetadata,
    private val onProjectionStopped: () -> Unit,
) {
    private val resourceLock = Any()
    private val closed = AtomicBoolean(false)
    private val handlerThread = HandlerThread("SGT-PhoneControl-Projection").apply { start() }
    private val handler = Handler(handlerThread.looper)
    private var dimensions = initialDimensions
    private var reader: ImageReader? = null
    private var virtualDisplay: VirtualDisplay? = null
    private var pendingCapture: CompletableDeferred<PhoneControlProjectionFrameResult>? = null
    private var cachedFrame: CachedProjectionFrame? = null
    private var firstFrameLogged = false
    private var consecutiveDecodeFailures = 0

    val isReady: Boolean
        get() = !closed.get() && synchronized(resourceLock) {
            reader != null && virtualDisplay != null
        }

    private val callback = object : MediaProjection.Callback() {
        override fun onStop() {
            if (close(requested = false)) {
                onProjectionStopped()
            }
        }

        override fun onCapturedContentResize(width: Int, height: Int) {
            if (width > 0 && height > 0) resize(width, height)
        }
    }

    fun start() {
        projection.registerCallback(callback, handler)
        synchronized(resourceLock) {
            check(!closed.get()) { "Projection stopped during startup." }
            val nextReader = createReader(dimensions)
            reader = nextReader
            virtualDisplay = requireNotNull(
                projection.createVirtualDisplay(
                    DISPLAY_NAME,
                    dimensions.width,
                    dimensions.height,
                    dimensions.densityDpi,
                    DisplayManager.VIRTUAL_DISPLAY_FLAG_AUTO_MIRROR,
                    nextReader.surface,
                    null,
                    handler,
                ),
            ) { "MediaProjection did not create a virtual display." }
        }
    }

    suspend fun capture(): PhoneControlProjectionFrameResult {
        if (!isReady) {
            return PhoneControlProjectionFrameResult.Failure(
                code = "projection_unavailable",
                retryable = false,
            )
        }
        val request = CompletableDeferred<PhoneControlProjectionFrameResult>()
        synchronized(resourceLock) {
            if (closed.get()) {
                return PhoneControlProjectionFrameResult.Failure(
                    code = "projection_unavailable",
                    retryable = false,
                )
            }
            if (pendingCapture != null) {
                return PhoneControlProjectionFrameResult.Failure(
                    code = "projection_capture_busy",
                    retryable = true,
                )
            }
            pendingCapture = request
        }
        val fresh = withTimeoutOrNull(FRESH_FRAME_WAIT_MS) { request.await() }
        if (fresh != null) return fresh
        return synchronized(resourceLock) {
            if (pendingCapture === request) pendingCapture = null
            cachedFrame?.copyForCaller()
                ?: PhoneControlProjectionFrameResult.Failure(
                    code = "projection_frame_unavailable",
                    retryable = true,
                )
        }
    }

    fun close(requested: Boolean): Boolean {
        if (!closed.compareAndSet(false, true)) return false
        val pending = synchronized(resourceLock) {
            val waiting = pendingCapture
            pendingCapture = null
            waiting
        }
        pending?.complete(
            PhoneControlProjectionFrameResult.Failure(
                code = "projection_stopped",
                retryable = false,
            ),
        )
        Log.i(
            TAG,
            "projection_session_stopped reason=${if (requested) "requested" else "platform"}",
        )
        val retirement = Runnable { retireResources(requested) }
        if (!handler.post(retirement)) {
            // This can happen only if the owned looper is already terminal. No
            // callback can still be admitted after that point.
            retirement.run()
        }
        return true
    }

    private fun retireResources(requested: Boolean) {
        val retired = synchronized(resourceLock) {
            val display = virtualDisplay
            val imageReader = reader
            val frame = cachedFrame
            virtualDisplay = null
            reader = null
            cachedFrame = null
            RetiredProjectionResources(display, imageReader, frame)
        }
        runCatching { retired.reader?.setOnImageAvailableListener(null, null) }
        runCatching { retired.display?.release() }
        runCatching { retired.reader?.close() }
        runCatching { retired.frame?.bitmap?.recycle() }
        runCatching { projection.unregisterCallback(callback) }
        if (requested) runCatching { projection.stop() }
        handlerThread.quitSafely()
    }

    private fun resize(width: Int, height: Int) {
        if (closed.get()) return
        synchronized(resourceLock) {
            if (closed.get() || dimensions.width == width && dimensions.height == height) return
            val next = dimensions.copy(width = width, height = height)
            val nextReader = createReader(next)
            val display = virtualDisplay
            if (display == null) {
                nextReader.close()
                return
            }
            display.resize(next.width, next.height, next.densityDpi)
            display.setSurface(nextReader.surface)
            val previous = reader
            reader = nextReader
            dimensions = next
            cachedFrame?.bitmap?.recycle()
            cachedFrame = null
            pendingCapture?.complete(
                PhoneControlProjectionFrameResult.Failure(
                    code = "projection_resized",
                    retryable = true,
                ),
            )
            pendingCapture = null
            previous?.setOnImageAvailableListener(null, null)
            previous?.close()
            Log.i(TAG, "projection_resized width=$width height=$height")
        }
    }

    private fun createReader(size: ProjectionDimensions): ImageReader =
        ImageReader.newInstance(
            size.width,
            size.height,
            PixelFormat.RGBA_8888,
            MAX_IMAGES,
        ).also { imageReader ->
            imageReader.setOnImageAvailableListener(::onImageAvailable, handler)
        }

    private fun onImageAvailable(source: ImageReader) {
        val image = runCatching { source.acquireLatestImage() }.getOrNull() ?: return
        val diagnostics = projectionImageDiagnostics(image)
        var decodedBitmap: Bitmap? = null
        try {
            val shouldCopy = synchronized(resourceLock) {
                !closed.get() && source === reader &&
                    (pendingCapture != null || cachedFrame == null)
            }
            if (!shouldCopy) return
            val bitmap = image.toBitmap()
            decodedBitmap = bitmap
            val capturedAtMs = SystemClock.elapsedRealtime()
            val metadata = displayMetadata()
            val published = synchronized(resourceLock) {
                if (closed.get() || source !== reader) {
                    bitmap.recycle()
                    decodedBitmap = null
                    return@synchronized null
                }
                val request = pendingCapture
                val callerCopy = request?.let {
                    bitmap.copy(Bitmap.Config.ARGB_8888, true)
                }
                cachedFrame?.bitmap?.recycle()
                cachedFrame = CachedProjectionFrame(bitmap, capturedAtMs, metadata)
                decodedBitmap = null
                pendingCapture = null
                if (!firstFrameLogged) {
                    firstFrameLogged = true
                    Log.i(TAG, "projection_frame_ready")
                }
                PublishedProjectionFrame(request, callerCopy)
            }
            if (published == null) return
            consecutiveDecodeFailures = 0
            published.request?.complete(
                PhoneControlProjectionFrameResult.Success(
                    requireNotNull(published.callerBitmap),
                    capturedAtMs,
                    metadata.rotation,
                    metadata.densityDpi,
                ),
            )
        } catch (error: Throwable) {
            runCatching { decodedBitmap?.recycle() }
            consecutiveDecodeFailures += 1
            if (shouldSummarizeProjectionDecodeFailure(consecutiveDecodeFailures)) {
                Log.e(
                    TAG,
                    "projection_frame_decode_failed width=${diagnostics.width} " +
                        "height=${diagnostics.height} " +
                        "pixel_stride=${diagnostics.pixelStride} " +
                        "row_stride=${diagnostics.rowStride} " +
                        "buffer_bytes=${diagnostics.bufferBytes} " +
                        "consecutive_failures=$consecutiveDecodeFailures",
                    error,
                )
            }
            val waiting = synchronized(resourceLock) {
                val request = pendingCapture
                pendingCapture = null
                request
            }
            waiting?.complete(
                PhoneControlProjectionFrameResult.Failure(
                    code = "projection_frame_decode_failed",
                    retryable = true,
                ),
            )
        } finally {
            runCatching { image.close() }
        }
    }

    private companion object {
        const val TAG = "SGTPhoneControlProjection"
        const val DISPLAY_NAME = "SGT Phone Control"
        const val MAX_IMAGES = 2
        const val FRESH_FRAME_WAIT_MS = 650L
    }
}

private data class RetiredProjectionResources(
    val display: VirtualDisplay?,
    val reader: ImageReader?,
    val frame: CachedProjectionFrame?,
)

private data class PublishedProjectionFrame(
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

private fun projectionImageDiagnostics(image: Image): ProjectionImageDiagnostics {
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

private data class CachedProjectionFrame(
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

private data class ProjectionDisplayMetadata(
    val rotation: Int,
    val densityDpi: Int,
)

private data class ProjectionDimensions(
    val width: Int,
    val height: Int,
    val densityDpi: Int,
)

private fun projectionDimensions(context: Context): ProjectionDimensions {
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

private fun projectionDisplayMetadata(context: Context): ProjectionDisplayMetadata {
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

private fun Image.toBitmap(): Bitmap {
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
