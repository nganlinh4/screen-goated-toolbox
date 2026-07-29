package dev.screengoated.toolbox.mobile.creation

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import java.io.File
import java.io.FileOutputStream
import java.io.InputStream
import java.nio.file.Files
import java.nio.file.LinkOption
import java.nio.file.StandardCopyOption
import java.security.MessageDigest
import java.util.UUID

internal class CreationPresentationPreviewStore(
    private val root: File,
    private val openInput: (String) -> InputStream,
) {
    @Synchronized
    fun materialize(source: String): String {
        root.mkdirs()
        val target = File(root, "${creationPresentationPreviewKey { openInput(source) }}.jpg")
        if (validCachedPreview(target)) {
            target.setLastModified(System.currentTimeMillis())
            return target.absolutePath
        }
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        openInput(source).use { BitmapFactory.decodeStream(it, null, bounds) }
        require(
            bounds.outWidth in 1..CreationContract.MAXIMUM_IMAGE_DIMENSION &&
                bounds.outHeight in 1..CreationContract.MAXIMUM_IMAGE_DIMENSION &&
                bounds.outWidth.toLong() * bounds.outHeight <=
                CreationContract.MAXIMUM_DECODED_IMAGE_PIXELS,
        ) { "Reference image is unavailable" }
        val sample = creationThumbnailSampleSize(
            bounds.outWidth,
            bounds.outHeight,
            MAXIMUM_PRESENTATION_EDGE,
        )
        val bitmap = requireNotNull(
            openInput(source).use {
                BitmapFactory.decodeStream(
                    it,
                    null,
                    BitmapFactory.Options().apply { inSampleSize = sample },
                )
            },
        ) { "Reference image is unavailable" }
        val pending = File(root, ".${target.name}.pending-${UUID.randomUUID()}")
        try {
            FileOutputStream(pending).use { output ->
                check(bitmap.compress(Bitmap.CompressFormat.JPEG, JPEG_QUALITY, output)) {
                    "Could not create reference preview"
                }
                output.fd.sync()
            }
            require(pending.length() in 1..MAXIMUM_PRESENTATION_BYTES) {
                "Reference preview is too large"
            }
            Files.move(
                pending.toPath(),
                target.toPath(),
                StandardCopyOption.ATOMIC_MOVE,
                StandardCopyOption.REPLACE_EXISTING,
            )
            require(validCachedPreview(target)) { "Could not create reference preview" }
            return target.absolutePath
        } finally {
            bitmap.recycle()
            pending.delete()
        }
    }

    private fun validCachedPreview(file: File): Boolean {
        val path = file.toPath()
        if (isCreationLink(path) ||
            !Files.isRegularFile(path, LinkOption.NOFOLLOW_LINKS) ||
            file.length() !in 1..MAXIMUM_PRESENTATION_BYTES
        ) return false
        val decoded = runCatching {
            file.inputStream().use(BitmapFactory::decodeStream)
        }.getOrNull() ?: return false
        return try {
            decoded.width in 1..MAXIMUM_PRESENTATION_EDGE &&
                decoded.height in 1..MAXIMUM_PRESENTATION_EDGE
        } finally {
            decoded.recycle()
        }
    }

    private companion object {
        const val MAXIMUM_PRESENTATION_EDGE = 512
        const val MAXIMUM_PRESENTATION_BYTES = 4L * 1024 * 1024
        const val JPEG_QUALITY = 86
    }
}

internal fun creationPresentationPreviewKey(openInput: () -> InputStream): String {
    val digest = MessageDigest.getInstance("SHA-256")
    digest.update("sgt-presentation-jpeg-v1".encodeToByteArray())
    var total = 0L
    openInput().use { input ->
        val buffer = ByteArray(DEFAULT_BUFFER_SIZE)
        while (true) {
            val read = input.read(buffer)
            if (read < 0) break
            total += read
            require(total <= CreationContract.MAXIMUM_SOURCE_IMAGE_BYTES) {
                "Reference image is too large"
            }
            digest.update(buffer, 0, read)
        }
    }
    require(total > 0L) { "Reference image is unavailable" }
    return digest.digest().joinToString("") { byte -> "%02x".format(byte) }
}

internal data class CreationPresentationArtifact(
    val path: String,
    val lastModifiedMs: Long,
    val sizeBytes: Long,
)

internal fun planCreationPresentationPrune(
    artifacts: List<CreationPresentationArtifact>,
    protectedPaths: Set<String>,
    nowMs: Long,
    maximumFiles: Int,
    maximumBytes: Long,
    retentionMs: Long,
): Set<String> {
    require(maximumFiles >= 0)
    require(maximumBytes >= 0L)
    require(retentionMs >= 0L)
    var retainedCount = artifacts.size
    var retainedBytes = artifacts.fold(0L) { total, artifact ->
        creationSaturatingBytes(total, artifact.sizeBytes.coerceAtLeast(0L))
    }
    return buildSet {
        artifacts.sortedBy(CreationPresentationArtifact::lastModifiedMs).forEach { artifact ->
            if (artifact.path in protectedPaths) return@forEach
            val expired = nowMs - artifact.lastModifiedMs >= retentionMs
            if (expired || retainedCount > maximumFiles || retainedBytes > maximumBytes) {
                add(artifact.path)
                retainedCount -= 1
                retainedBytes = (retainedBytes - artifact.sizeBytes.coerceAtLeast(0L))
                    .coerceAtLeast(0L)
            }
        }
    }
}
