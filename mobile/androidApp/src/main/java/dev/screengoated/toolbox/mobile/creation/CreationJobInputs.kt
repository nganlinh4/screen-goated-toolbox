package dev.screengoated.toolbox.mobile.creation

import java.io.File
import java.io.InputStream
import java.nio.file.Files
import java.nio.file.LinkOption
import java.nio.file.StandardCopyOption
import java.util.UUID

internal class CreationJobInputStore(
    private val root: File,
    private val openInput: (String) -> InputStream,
    private val linkInput: (String, File) -> Boolean,
) {
    fun materialize(
        jobId: String,
        sourceHandles: List<String>,
        tool: CreationTool,
    ): List<String> = materializeCreationJobInputs(
        root.apply(File::mkdirs),
        jobId,
        sourceHandles,
        if (tool == CreationTool.IMAGE_CREATOR) {
            CreationContract.MAXIMUM_IMAGE_REFERENCE_AGGREGATE_BYTES
        } else {
            CreationContract.MAXIMUM_SOURCE_IMAGE_BYTES
        },
        openInput,
        linkInput,
    )

    fun release(paths: List<String>): Boolean {
        val directories = paths.mapNotNull { path ->
            File(path).parentFile?.takeIf { it.parentFile?.absoluteFile == root.absoluteFile }
        }.distinctBy(File::getAbsolutePath)
        return directories.all { directory ->
            !directory.exists() || deleteCreationTreeNoFollow(root, directory)
        }
    }
}

internal fun materializeCreationJobInputs(
    root: File,
    jobId: String,
    sourceHandles: List<String>,
    maximumAggregateBytes: Long,
    openInput: (String) -> InputStream,
    linkInput: (String, File) -> Boolean = { _, _ -> false },
): List<String> {
    require(jobId.matches(Regex("[a-z0-9_-]{1,160}")))
    if (sourceHandles.isEmpty()) return emptyList()
    val directory = File(root, jobId)
    require(!directory.exists() && directory.mkdirs()) { "Could not reserve job inputs" }
    val outputs = mutableListOf<String>()
    var aggregate = 0L
    try {
        sourceHandles.forEachIndexed { index, handle ->
            val remaining = maximumAggregateBytes - aggregate
            require(remaining > 0L) { "Selected images exceed the job input limit" }
            val pending = File(directory, ".$index.pending-${UUID.randomUUID()}")
            try {
                val copied = if (linkInput(handle, pending)) {
                    pending.length()
                } else {
                    val bytes = openInput(handle).use {
                        copyCreationInputBounded(
                            it,
                            pending,
                            minOf(CreationContract.MAXIMUM_SOURCE_IMAGE_BYTES, remaining),
                        )
                    }
                    bytes
                }
                require(copied <= remaining) { "Selected images exceed the job input limit" }
                val mimeType = validateImportedCreationImage(pending)
                val target = File(directory, "$index.${creationImageExtension(mimeType)}")
                Files.move(pending.toPath(), target.toPath(), StandardCopyOption.ATOMIC_MOVE)
                aggregate += copied
                outputs += target.absolutePath
            } finally {
                pending.delete()
            }
        }
        return outputs
    } catch (failure: Throwable) {
        deleteCreationTreeNoFollow(root, directory)
        throw failure
    }
}

internal fun creationAcceptedInputCanLink(filesDir: File, sourcePath: String): Boolean {
    val source = runCatching { File(sourcePath).canonicalFile }.getOrNull() ?: return false
    val allowed = listOf(
        File(filesDir, "creation/sources").canonicalFile,
        File(filesDir, "creation/job-inputs").canonicalFile,
    ).any { root -> source.toPath().startsWith(root.toPath()) }
    return allowed &&
        !isCreationLink(source.toPath()) &&
        Files.isRegularFile(source.toPath(), LinkOption.NOFOLLOW_LINKS)
}

internal fun linkCreationAcceptedInput(
    filesDir: File,
    sourcePath: String,
    target: File,
): Boolean {
    if (!creationAcceptedInputCanLink(filesDir, sourcePath)) return false
    val source = File(sourcePath).canonicalFile
    return runCatching {
        Files.createLink(target.toPath(), source.toPath())
        true
    }.getOrDefault(false)
}
