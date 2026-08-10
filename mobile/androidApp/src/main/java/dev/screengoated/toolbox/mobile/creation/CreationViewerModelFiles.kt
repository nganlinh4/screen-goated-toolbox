package dev.screengoated.toolbox.mobile.creation

import java.io.File
import java.util.UUID

internal class CreationViewerModelFiles(private val files: CreationFileStore) {
    private val cache = CreationPreviewCache()
    private val directory = File(files.context.cacheDir, "creation/model-viewer")

    fun materialize(path: String): File {
        val source = files.materializePreview(path, "glb")
        val target = cache.materialize(
            directory = directory,
            key = UUID.randomUUID().toString().replace("-", ""),
            extension = "glb",
            maximumBytes = CreationContract.MAXIMUM_GLB_ARTIFACT_BYTES,
            reusable = false,
            openInput = source::inputStream,
            validate = CreationArtifactValidator::validateGlb,
        )
        prune(protected = target)
        return target
    }

    fun release(file: File): Boolean =
        !file.exists() || deleteCreationFileConfined(directory.canonicalFile, file)

    private fun prune(protected: File) {
        val candidates = creationRegularFilesNoFollow(directory).sortedBy(File::lastModified)
        var retainedCount = candidates.size
        var retainedBytes = candidates.sumOf { it.length().coerceAtLeast(0L) }
        val now = System.currentTimeMillis()
        candidates.forEach { candidate ->
            if (candidate == protected) return@forEach
            if (
                now - candidate.lastModified() >= RETENTION_MS ||
                retainedCount > MAXIMUM_FILES ||
                retainedBytes > MAXIMUM_BYTES
            ) {
                val length = candidate.length().coerceAtLeast(0L)
                if (deleteCreationFileConfined(directory, candidate)) {
                    retainedCount -= 1
                    retainedBytes -= length
                }
            }
        }
    }

    private companion object {
        const val MAXIMUM_FILES = 4
        const val MAXIMUM_BYTES = 256L * 1024 * 1024
        const val RETENTION_MS = 24L * 60 * 60 * 1_000
    }
}
