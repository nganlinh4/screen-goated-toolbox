package dev.screengoated.toolbox.mobile.phonecontrol.projection

import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.PixelFormat
import android.hardware.display.DisplayManager
import android.hardware.display.VirtualDisplay
import android.media.ImageReader
import android.media.projection.MediaProjection
import android.media.projection.MediaProjectionManager
import android.os.Build
import android.os.Handler
import android.os.HandlerThread
import android.os.SystemClock
import android.view.Display
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
            displayManager = context.getSystemService(DisplayManager::class.java),
            currentDimensions = { projectionDimensions(context) },
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
    private val displayManager: DisplayManager?,
    private val currentDimensions: () -> ProjectionDimensions,
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

    // The newest frame that was not decoded yet. AUTO_MIRROR delivers frames
    // only on change, so the last frame of a change must stay available for a
    // capture that finds the screen static afterwards.
    private var latestImage: RetainedProjectionImage? = null
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

    // Below API 34 the platform reports no captured-content resize, so a
    // rotation is followed through the default display instead.
    private val displayListener = object : DisplayManager.DisplayListener {
        override fun onDisplayAdded(displayId: Int) = Unit

        override fun onDisplayRemoved(displayId: Int) = Unit

        override fun onDisplayChanged(displayId: Int) {
            if (displayId != Display.DEFAULT_DISPLAY) return
            val next = runCatching(currentDimensions).getOrNull() ?: return
            val current = synchronized(resourceLock) { dimensions }
            if (shouldFollowDisplayResize(Build.VERSION.SDK_INT, current.width, current.height, next.width, next.height)) {
                resize(next.width, next.height)
            }
        }
    }

    fun start() {
        projection.registerCallback(callback, handler)
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            displayManager?.registerDisplayListener(displayListener, handler)
        }
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
            refreshCacheFromRetainedImageLocked()
            cachedFrame?.copyForCaller()
                ?: PhoneControlProjectionFrameResult.Failure(
                    code = "projection_frame_unavailable",
                    retryable = true,
                )
        }
    }

    /** Decodes the newest undecoded frame into the cache. Caller holds [resourceLock]. */
    private fun refreshCacheFromRetainedImageLocked() {
        val retained = latestImage ?: return
        latestImage = null
        try {
            if (closed.get()) return
            val bitmap = retained.image.toBitmap()
            cachedFrame?.bitmap?.recycle()
            cachedFrame = CachedProjectionFrame(bitmap, retained.capturedAtMs, displayMetadata())
        } catch (error: Throwable) {
            Log.e(TAG, "projection_retained_frame_decode_failed", error)
        } finally {
            runCatching { retained.image.close() }
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
            latestImage?.let { runCatching { it.image.close() } }
            latestImage = null
            RetiredProjectionResources(display, imageReader, frame)
        }
        runCatching { displayManager?.unregisterDisplayListener(displayListener) }
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
            latestImage?.let { runCatching { it.image.close() } }
            latestImage = null
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
        var retained = false
        try {
            val disposition = synchronized(resourceLock) {
                val next = projectionFrameDisposition(
                    active = !closed.get() && source === reader,
                    captureWaiting = pendingCapture != null,
                    cacheEmpty = cachedFrame == null,
                )
                if (next == ProjectionFrameDisposition.RETAIN) {
                    latestImage?.let { runCatching { it.image.close() } }
                    latestImage = RetainedProjectionImage(image, SystemClock.elapsedRealtime())
                    retained = true
                }
                next
            }
            if (disposition != ProjectionFrameDisposition.DECODE) return
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
                // Anything retained earlier is older than this decoded frame.
                latestImage?.let { runCatching { it.image.close() } }
                latestImage = null
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
            if (!retained) runCatching { image.close() }
        }
    }

    private companion object {
        const val TAG = "SGTPhoneControlProjection"
        const val DISPLAY_NAME = "SGT Phone Control"
        // One retained newest frame plus the two that acquireLatestImage needs.
        const val MAX_IMAGES = 3
        const val FRESH_FRAME_WAIT_MS = 650L
    }
}
