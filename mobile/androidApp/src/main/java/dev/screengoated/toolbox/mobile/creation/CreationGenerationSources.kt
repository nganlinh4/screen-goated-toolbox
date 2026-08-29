package dev.screengoated.toolbox.mobile.creation

import java.io.File
import java.nio.file.Files
import java.nio.file.LinkOption

internal fun creationGenerationSourcesAreUsable(
    filesDir: File,
    leasedPaths: Set<String>,
    requestedPaths: List<String>,
    exists: (String) -> Boolean,
): Boolean = requestedPaths.all { path ->
    path in leasedPaths &&
        exists(path) &&
        (
            path.startsWith("content://") ||
                creationGenerationSourceIsLocalOriginal(filesDir, path)
            )
}

private fun creationGenerationSourceIsLocalOriginal(filesDir: File, path: String): Boolean {
    val requested = File(path)
    if (!requested.isAbsolute) return false
    val unresolved = requested.toPath().toAbsolutePath().normalize()
    if (isCreationLink(unresolved) ||
        !Files.isRegularFile(unresolved, LinkOption.NOFOLLOW_LINKS)
    ) return false
    val sourceRoot = runCatching {
        File(filesDir, "creation/sources").canonicalFile.toPath()
    }.getOrNull() ?: return false
    val resolved = runCatching { requested.canonicalFile.toPath() }.getOrNull() ?: return false
    return resolved != sourceRoot && resolved.startsWith(sourceRoot)
}
