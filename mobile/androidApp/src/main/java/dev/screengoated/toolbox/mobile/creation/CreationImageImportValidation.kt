package dev.screengoated.toolbox.mobile.creation

import android.graphics.BitmapFactory
import java.io.File
import java.io.FileOutputStream
import java.io.InputStream

internal fun copyCreationInputBounded(input: InputStream, target: File, maximumBytes: Long): Long {
    require(maximumBytes >= 0L) { "Import limit is invalid" }
    var copied = 0L
    try {
        FileOutputStream(target).use { output ->
            val buffer = ByteArray(DEFAULT_BUFFER_SIZE)
            while (true) {
                val read = input.read(buffer)
                if (read < 0) break
                copied += read
                require(copied <= maximumBytes) { "Selected image is too large" }
                output.write(buffer, 0, read)
            }
            output.fd.sync()
        }
        return copied
    } catch (error: Throwable) {
        target.delete()
        throw error
    }
}

internal fun readCreationBytesBounded(input: InputStream, maximumBytes: Long): ByteArray {
    require(maximumBytes in 0..Int.MAX_VALUE.toLong())
    val output = java.io.ByteArrayOutputStream(minOf(maximumBytes, 64 * 1024L).toInt())
    val buffer = ByteArray(DEFAULT_BUFFER_SIZE)
    var total = 0L
    while (true) {
        val read = input.read(buffer)
        if (read < 0) break
        total += read
        require(total <= maximumBytes) { "Preview asset is too large" }
        output.write(buffer, 0, read)
    }
    return output.toByteArray()
}

internal fun validateImportedCreationImage(file: File) {
    val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
    BitmapFactory.decodeFile(file.absolutePath, bounds)
    require(bounds.outWidth > 0 && bounds.outHeight > 0) { "Selected file is not an image" }
    require(
        bounds.outWidth <= CreationContract.MAXIMUM_IMAGE_DIMENSION &&
            bounds.outHeight <= CreationContract.MAXIMUM_IMAGE_DIMENSION,
    ) {
        "Selected image dimensions are too large"
    }
    require(
        bounds.outWidth.toLong() * bounds.outHeight <=
            CreationContract.MAXIMUM_DECODED_IMAGE_PIXELS,
    ) {
        "Selected image is larger than 64 megapixels"
    }
    require(
        bounds.outMimeType in setOf("image/png", "image/jpeg", "image/webp"),
    ) { "Selected image type is not supported" }
    var sample = 1
    while (bounds.outWidth / sample > 2_048 || bounds.outHeight / sample > 2_048) sample *= 2
    requireNotNull(
        BitmapFactory.decodeFile(
            file.absolutePath,
            BitmapFactory.Options().apply { inSampleSize = sample },
        ),
    ) { "Selected file is not a complete image" }.recycle()
}
