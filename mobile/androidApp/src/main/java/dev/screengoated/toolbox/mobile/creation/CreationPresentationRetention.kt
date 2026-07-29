package dev.screengoated.toolbox.mobile.creation

import java.io.File

internal fun pruneCreationPresentationArtifacts(
    filesDir: File,
    cleanup: CreationPendingCleanupStore,
) {
    check(creationDurableStateIsReadable(filesDir)) {
        CREATION_STORAGE_UNAVAILABLE_ERROR_KEY
    }
    val root = File(filesDir, "creation/presentation")
    val protected = creationDurableProtectedPaths(filesDir)
        .mapNotNull { path -> runCatching { File(path).canonicalPath }.getOrNull() }
        .toSet()
    val files = creationRegularFilesNoFollow(root)
    val byPath = files.associateBy { it.canonicalPath }
    val planned = planCreationPresentationPrune(
        artifacts = byPath.map { (path, file) ->
            CreationPresentationArtifact(path, file.lastModified(), file.length())
        },
        protectedPaths = protected,
        nowMs = System.currentTimeMillis(),
        maximumFiles = MAXIMUM_PRESENTATION_FILES,
        maximumBytes = MAXIMUM_PRESENTATION_BYTES,
        retentionMs = PRESENTATION_RETENTION_MS,
    )
    cleanup.isolateAndEnqueue(
        planned.mapNotNull { path ->
            byPath[path]?.let { CreationCleanupCandidate.trustedManaged(it.absolutePath) }
        },
    )
    cleanup.drain()
}

internal const val MAXIMUM_PRESENTATION_FILES = 512
internal const val MAXIMUM_PRESENTATION_BYTES = 256L * 1024 * 1024
internal const val PRESENTATION_RETENTION_MS = 7L * 24 * 60 * 60 * 1_000
