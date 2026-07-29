package dev.screengoated.toolbox.mobile.creation

import java.io.File
import java.nio.file.Files
import java.nio.file.LinkOption
import java.util.UUID

internal fun reserveCreationStagingFile(directory: File, requestedName: String): File {
    directory.mkdirs()
    require(
        Files.isDirectory(directory.toPath(), LinkOption.NOFOLLOW_LINKS) &&
            !isCreationLink(directory.toPath()),
    ) { "Creation staging is unavailable" }
    val requested = File(requestedName).name
    val dot = requested.lastIndexOf('.')
    val stem = if (dot > 0) requested.substring(0, dot) else requested
    val extension = if (dot > 0) requested.substring(dot) else ""
    repeat(MAXIMUM_STAGING_NAME_ATTEMPTS) { index ->
        val name = if (index == 0) requested else "${stem}_${index + 1}$extension"
        val candidate = File(directory, name)
        if (candidate.createNewFile()) return candidate
    }
    val fallback = File(directory, "$stem-${UUID.randomUUID()}$extension")
    check(fallback.createNewFile()) { "Could not reserve output file" }
    return fallback
}

internal fun sealReservedCreationStagingFile(
    filesDir: File,
    tool: CreationTool,
    path: String,
): File {
    val root = creationStagingRoot(filesDir, tool)
    return requireNotNull(sealCreationStagingFile(root, File(path))) {
        "Creation returned an invalid output file"
    }
}

internal fun isReservedCreationStagingFile(
    filesDir: File,
    tool: CreationTool,
    path: String,
): Boolean {
    val root = creationStagingRoot(filesDir, tool)
    val candidate = File(path)
    return isReservedCreationStagingPath(filesDir, tool, path) &&
        isCreationRegularFileConfined(root, candidate)
}

internal fun isReservedCreationStagingPath(
    filesDir: File,
    tool: CreationTool,
    path: String,
): Boolean = File(path).toPath().toAbsolutePath().normalize().parent ==
    creationStagingRoot(filesDir, tool).toPath().toAbsolutePath().normalize()

internal fun isManagedCreationJobInput(
    filesDir: File,
    jobId: String,
    path: String,
): Boolean {
    val root = File(filesDir, "creation/job-inputs/$jobId")
    val candidate = File(path)
    return candidate.toPath().toAbsolutePath().normalize().parent ==
        root.toPath().toAbsolutePath().normalize() &&
        isCreationRegularFileConfined(root, candidate)
}

internal fun deleteReservedCreationStagingFile(
    filesDir: File,
    tool: CreationTool,
    path: String,
): Boolean {
    val root = creationStagingRoot(filesDir, tool)
    val candidate = File(path)
    val rootPath = root.toPath().toAbsolutePath().normalize()
    val candidatePath = candidate.toPath().toAbsolutePath().normalize()
    if (candidatePath.parent != rootPath) return false
    if (!Files.exists(candidatePath, LinkOption.NOFOLLOW_LINKS)) return true
    return deleteCreationFileConfined(root, candidate)
}

private fun creationStagingRoot(filesDir: File, tool: CreationTool): File =
    File(filesDir, "creation/staging/${tool.wireName}")

private const val MAXIMUM_STAGING_NAME_ATTEMPTS = 10_000
