package dev.screengoated.toolbox.mobile.creation

import java.io.File
import java.nio.file.Files
import java.nio.file.StandardCopyOption
import java.util.UUID

internal fun creationManagedArtifactRoots(filesDir: File, cacheDir: File): List<File> = listOf(
    File(filesDir, "creation/sources"),
    File(filesDir, "creation/job-inputs"),
    File(filesDir, "creation/library"),
    File(filesDir, "creation/presentation"),
    File(filesDir, "creation/relinquished"),
    File(filesDir, "creation/staging"),
    File(cacheDir, "creation/previews"),
)

internal fun isUserOwnedCreationOutputPath(path: String): Boolean = path.startsWith("content://")

internal fun publishManagedCreationResult(
    filesDir: File,
    source: File,
    requestedName: String,
): File {
    val directory = File(filesDir, "creation/library").apply(File::mkdirs)
    require(isCreationRegularFileConfined(requireNotNull(directory.parentFile), source)) {
        "Creation result is unavailable"
    }
    val target = synchronized(creationManagedPublishLock) {
        val requested = File(requestedName).name.ifBlank { "result" }
        var candidate = File(directory, requested)
        while (candidate.exists()) {
            val dot = requested.lastIndexOf('.')
            val stem = requested.substring(0, dot.takeIf { it > 0 } ?: requested.length)
            val extension = requested.substring(dot.takeIf { it > 0 } ?: requested.length)
            candidate = File(directory, "$stem-${UUID.randomUUID()}$extension")
        }
        Files.move(source.toPath(), candidate.toPath(), StandardCopyOption.ATOMIC_MOVE)
        candidate
    }
    require(isCreationRegularFileConfined(directory, target)) {
        "Creation result could not be committed"
    }
    return target
}

private val creationManagedPublishLock = Any()
