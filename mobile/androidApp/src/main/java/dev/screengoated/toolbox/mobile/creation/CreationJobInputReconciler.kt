package dev.screengoated.toolbox.mobile.creation

import java.io.File
import java.nio.file.Files
import java.nio.file.LinkOption

internal data class CreationJobInputDirectory(
    val path: String,
    val lastModifiedMs: Long,
)

internal fun planCreationJobInputReconciliation(
    directories: List<CreationJobInputDirectory>,
    ownedPaths: Set<String>,
    nowMs: Long,
    graceMs: Long,
    maximumDeletes: Int,
): List<String> {
    require(graceMs >= 0L)
    require(maximumDeletes >= 0)
    return directories.asSequence()
        .filter { directory ->
            directory.path !in ownedPaths &&
                nowMs - directory.lastModifiedMs >= graceMs
        }
        .sortedBy(CreationJobInputDirectory::lastModifiedMs)
        .take(maximumDeletes)
        .map(CreationJobInputDirectory::path)
        .toList()
}

internal fun CreationFileStore.reconcileJobInputOwnership() {
    repeat(MAXIMUM_RECONCILE_PASSES) {
        if (!reconcileCreationJobInputDirectories(context.filesDir)) return
    }
}

internal fun reconcileCreationJobInputDirectories(
    filesDir: File,
    nowMs: Long = System.currentTimeMillis(),
): Boolean {
    check(creationDurableStateIsReadable(filesDir)) {
        CREATION_STORAGE_UNAVAILABLE_ERROR_KEY
    }
    val root = File(filesDir, "creation/job-inputs")
    val rootPath = root.toPath().toAbsolutePath().normalize()
    if (!Files.isDirectory(rootPath, LinkOption.NOFOLLOW_LINKS)) return false
    check(!isCreationLink(rootPath)) { CREATION_STORAGE_UNAVAILABLE_ERROR_KEY }
    val owned = creationDurableProtectedPaths(filesDir).mapNotNull { protected ->
        val candidate = runCatching { File(protected).canonicalFile }.getOrNull()
            ?: return@mapNotNull null
        candidate.takeIf {
            it.toPath().startsWith(rootPath) && it.toPath() != rootPath
        }?.toPath()?.let { rootPath.relativize(it).getName(0) }
            ?.let(rootPath::resolve)
            ?.toFile()
            ?.canonicalPath
    }.toSet()
    val directories = mutableListOf<CreationJobInputDirectory>()
    Files.newDirectoryStream(rootPath).use { entries ->
        for (entry in entries) {
            if (Files.isDirectory(entry, LinkOption.NOFOLLOW_LINKS) &&
                !isCreationLink(entry) &&
                entry.parent == rootPath &&
                entry.toFile().canonicalPath !in owned &&
                nowMs - entry.toFile().lastModified() >= JOB_INPUT_ORPHAN_GRACE_MS &&
                directories.size < MAXIMUM_RECONCILE_DELETES
            ) {
                directories += CreationJobInputDirectory(
                    entry.toFile().canonicalPath,
                    entry.toFile().lastModified(),
                )
            }
        }
    }
    val planned = planCreationJobInputReconciliation(
        directories,
        owned,
        nowMs,
        JOB_INPUT_ORPHAN_GRACE_MS,
        MAXIMUM_RECONCILE_DELETES,
    )
    planned.forEach { path ->
        check(deleteCreationTreeNoFollow(root, File(path))) {
            CREATION_STORAGE_UNAVAILABLE_ERROR_KEY
        }
    }
    return planned.size == MAXIMUM_RECONCILE_DELETES
}

internal const val JOB_INPUT_ORPHAN_GRACE_MS = 10L * 60 * 1_000
private const val MAXIMUM_RECONCILE_DELETES = 4_096
private const val MAXIMUM_RECONCILE_PASSES = 4
