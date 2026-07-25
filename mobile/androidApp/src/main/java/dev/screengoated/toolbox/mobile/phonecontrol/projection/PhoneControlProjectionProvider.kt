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
        val dimensions = projectionDimensions(context)
        val candidate = ProjectionSession(
            projection = projection,
            initialDimensions = dimensions,
            displayMetadata = { projectionDisplayMetadata(context) },
            onProjectionStopped = onProjectionStopped,
        )
        return try {
            candidate.start()
            synchronized(lock) { session = candidate }
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

    val isReady: Boolean
        get() = !closed.get() && synchronized(resourceLock) {
            reader != null && virtualDisplay != null
        }

    private val callback = object : MediaProjection.Callback() {
        override fun onStop() {
            if (close(requested = false)) {
                Log.w(TAG, "projection_session_stopped reason=platform")
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
            val nextReader = createReader(dimensions)
            reader = nextReader
            virtualDisplay = projection.createVirtualDisplay(
                DISPLAY_NAME,
                dimensions.width,
                dimensions.height,
                dimensions.densityDpi,
                DisplayManager.VIRTUAL_DISPLAY_FLAG_AUTO_MIRROR,
                nextReader.surface,
                null,
                handler,
            )
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
            virtualDisplay?.release()
            virtualDisplay = null
            reader?.setOnImageAvailableListener(null, null)
            reader?.close()
            reader = null
            cachedFrame?.bitmap?.recycle()
            cachedFrame = null
            waiting
        }
        pending?.complete(
            PhoneControlProjectionFrameResult.Failure(
                code = "projection_stopped",
                retryable = false,
            ),
        )
        runCatching { projection.unregisterCallback(callback) }
        if (requested) runCatching { projection.stop() }
        handlerThread.quitSafely()
        return true
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
        try {
            val shouldCopy = synchronized(resourceLock) {
                !closed.get() && source === reader &&
                    (pendingCapture != null || cachedFrame == null)
            }
            if (!shouldCopy) return
            val bitmap = image.toBitmap()
            val capturedAtMs = SystemClock.elapsedRealtime()
            val metadata = displayMetadata()
            val waiting = synchronized(resourceLock) {
                if (closed.get() || source !== reader) {
                    bitmap.recycle()
                    return@synchronized null
                }
                cachedFrame?.bitmap?.recycle()
                cachedFrame = CachedProjectionFrame(bitmap, capturedAtMs, metadata)
                val request = pendingCapture
                pendingCapture = null
                if (!firstFrameLogged) {
                    firstFrameLogged = true
                    Log.i(TAG, "projection_frame_ready")
                }
                request
            }
            waiting?.complete(
                PhoneControlProjectionFrameResult.Success(
                    bitmap.copy(Bitmap.Config.ARGB_8888, true),
                    capturedAtMs,
                    metadata.rotation,
                    metadata.densityDpi,
                ),
            )
        } catch (_: Throwable) {
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
            image.close()
        }
    }

    private companion object {
        const val TAG = "SGTPhoneControlProjection"
        const val DISPLAY_NAME = "SGT Phone Control"
        const val MAX_IMAGES = 2
        const val FRESH_FRAME_WAIT_MS = 650L
    }
}

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
    val windowManager = context.getSystemService(WindowManager::class.java)
    @Suppress("DEPRECATION")
    val rotation = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
        context.display.rotation
    } else {
        windowManager.defaultDisplay.rotation
    }
    return ProjectionDisplayMetadata(
        rotation = rotation,
        densityDpi = context.resources.configuration.densityDpi.coerceAtLeast(1),
    )
}

private fun Image.toBitmap(): Bitmap {
    val plane = planes.firstOrNull() ?: error("Projection image has no pixel plane")
    val pixelStride = plane.pixelStride
    val rowStride = plane.rowStride
    require(pixelStride > 0 && rowStride >= width * pixelStride)
    val paddedWidth = rowStride / pixelStride
    val padded = Bitmap.createBitmap(paddedWidth, height, Bitmap.Config.ARGB_8888)
    plane.buffer.rewind()
    padded.copyPixelsFromBuffer(plane.buffer)
    if (paddedWidth == width) return padded
    return Bitmap.createBitmap(padded, 0, 0, width, height).also { padded.recycle() }
}
