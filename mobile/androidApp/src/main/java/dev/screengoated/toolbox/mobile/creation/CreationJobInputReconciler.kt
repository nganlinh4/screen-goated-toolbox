package dev.screengoated.toolbox.mobile.creation

import java.io.File
import java.nio.file.Files
import java.nio.file.FileVisitResult
import java.nio.file.LinkOption
import java.nio.file.Path
import java.nio.file.SimpleFileVisitor
import java.nio.file.attribute.BasicFileAttributes

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
    val reconciled = planned.count { path ->
        deleteCreationJobInputOrConfirmAbsent(root, File(path))
    }
    return reconciled == MAXIMUM_RECONCILE_DELETES
}

internal fun deleteCreationJobInputOrConfirmAbsent(root: File, target: File): Boolean =
    deleteCreationTreeNoFollow(root, target) ||
        deleteCreationJobInputTreeNoFollow(root, target) ||
        Files.notExists(target.toPath(), LinkOption.NOFOLLOW_LINKS)

private fun deleteCreationJobInputTreeNoFollow(root: File, target: File): Boolean {
    val rootPath = root.toPath().toAbsolutePath().normalize()
    val targetPath = target.toPath().toAbsolutePath().normalize()
    if (targetPath.parent != rootPath ||
        isCreationLink(rootPath) ||
        isCreationLink(targetPath) ||
        !Files.isDirectory(targetPath, LinkOption.NOFOLLOW_LINKS)
    ) return false
    var complete = true
    runCatching {
        Files.walkFileTree(
            targetPath,
            object : SimpleFileVisitor<Path>() {
                override fun visitFile(
                    file: Path,
                    attrs: BasicFileAttributes,
                ): FileVisitResult {
                    if (file.parent != targetPath ||
                        isCreationLink(file) ||
                        !Files.isRegularFile(file, LinkOption.NOFOLLOW_LINKS)
                    ) {
                        complete = false
                        return FileVisitResult.TERMINATE
                    }
                    complete = Files.deleteIfExists(file) && complete
                    return FileVisitResult.CONTINUE
                }

                override fun postVisitDirectory(
                    dir: Path,
                    error: java.io.IOException?,
                ): FileVisitResult {
                    complete = error == null &&
                        dir == targetPath &&
                        Files.deleteIfExists(dir) &&
                        complete
                    return FileVisitResult.CONTINUE
                }
            },
        )
    }.onFailure { complete = false }
    return complete
}

internal const val JOB_INPUT_ORPHAN_GRACE_MS = 10L * 60 * 1_000
private const val MAXIMUM_RECONCILE_DELETES = 4_096
private const val MAXIMUM_RECONCILE_PASSES = 4
