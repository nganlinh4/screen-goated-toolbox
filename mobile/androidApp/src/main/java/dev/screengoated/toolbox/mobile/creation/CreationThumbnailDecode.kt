package dev.screengoated.toolbox.mobile.creation

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.net.Uri
import java.io.InputStream
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.sync.Semaphore
import kotlinx.coroutines.sync.withPermit
import kotlinx.coroutines.withContext
import kotlin.coroutines.coroutineContext

private val creationThumbnailDecodePermits = Semaphore(2)

internal fun creationThumbnailSampleSize(
    width: Int,
    height: Int,
    maximumEdgePixels: Int,
): Int {
    require(width > 0 && height > 0)
    require(maximumEdgePixels > 0)
    var sample = 1
    while (ceilingDivide(width, sample) > maximumEdgePixels ||
        ceilingDivide(height, sample) > maximumEdgePixels
    ) {
        require(sample <= Int.MAX_VALUE / 2) { "Image dimensions are too large" }
        sample *= 2
    }
    return sample
}

internal suspend fun decodeCreationThumbnail(
    context: Context,
    path: String,
    maximumEdgePixels: Int,
): Bitmap? = decodeCreationResourceCancellationSafe(Dispatchers.IO, Bitmap::recycle) {
    creationThumbnailDecodePermits.withPermit {
        decodeCreationThumbnailOwned(context, path, maximumEdgePixels)
    }
}

internal suspend fun <T> decodeCreationResourceCancellationSafe(
    dispatcher: CoroutineDispatcher,
    dispose: (T) -> Unit,
    decode: suspend () -> T?,
): T? {
    var owned: T? = null
    try {
        val result = withContext(dispatcher) { decode().also { owned = it } }
        coroutineContext.ensureActive()
        owned = null
        return result
    } finally {
        owned?.let(dispose)
    }
}

private fun decodeCreationThumbnailOwned(
    context: Context,
    path: String,
    maximumEdgePixels: Int,
): Bitmap? = runCatching {
    require(maximumEdgePixels in 1..MAXIMUM_THUMBNAIL_EDGE)
    val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
    openCreationThumbnailInput(context, path).use { input ->
        BitmapFactory.decodeStream(input, null, bounds)
    }
    require(
        bounds.outWidth in 1..CreationContract.MAXIMUM_IMAGE_DIMENSION &&
            bounds.outHeight in 1..CreationContract.MAXIMUM_IMAGE_DIMENSION,
    )
    require(
        bounds.outWidth.toLong() * bounds.outHeight <= CreationContract.MAXIMUM_DECODED_IMAGE_PIXELS,
    )
    val sample = creationThumbnailSampleSize(bounds.outWidth, bounds.outHeight, maximumEdgePixels)
    openCreationThumbnailInput(context, path).use { input ->
        BitmapFactory.decodeStream(
            input,
            null,
            BitmapFactory.Options().apply { inSampleSize = sample },
        )
    }?.also { bitmap ->
        if (bitmap.width > maximumEdgePixels || bitmap.height > maximumEdgePixels) {
            bitmap.recycle()
            error("Thumbnail decode exceeded its memory bound")
        }
    }
}.getOrNull()

private fun openCreationThumbnailInput(context: Context, path: String): InputStream =
    if (path.startsWith("content://")) {
        requireNotNull(context.contentResolver.openInputStream(Uri.parse(path))) {
            "Reference image is unavailable"
        }
    } else {
        java.io.File(path).inputStream()
    }

private fun ceilingDivide(value: Int, divisor: Int): Int =
    ((value.toLong() + divisor - 1) / divisor).toInt()

private const val MAXIMUM_THUMBNAIL_EDGE = 2_048
