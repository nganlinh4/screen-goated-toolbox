package dev.screengoated.toolbox.mobile.creation

import java.io.File
import java.nio.file.Files
import java.nio.file.LinkOption
import java.nio.file.attribute.BasicFileAttributes

internal data class CreationManagedStorageSnapshot(
    val totalBytes: Long,
    val protectedBytes: Long,
)

internal fun snapshotCreationManagedStorage(
    roots: List<File>,
    protectedPaths: Set<String>,
): CreationManagedStorageSnapshot {
    val uniqueRoots = distinctCreationManagedRoots(roots)
    val protected = protectedPaths.mapNotNull { path ->
        runCatching { File(path).canonicalPath }.getOrNull()
    }.toSet()
    val candidates = uniqueRoots.flatMap(::creationRegularFilesNoFollow)
        .distinctBy(File::getAbsolutePath)
    val byPhysicalFile = groupCreationPhysicalFiles(candidates)
    val protectedPhysicalFiles = candidates.asSequence()
        .filter { it.canonicalPath in protected }
        .mapNotNull { protectedFile ->
            byPhysicalFile.entries.firstOrNull { (_, links) ->
                links.any { Files.isSameFile(it.toPath(), protectedFile.toPath()) }
            }?.key
        }
        .toSet()
    return CreationManagedStorageSnapshot(
        totalBytes = byPhysicalFile.values.fold(0L) { total, links ->
            creationSaturatingBytes(total, links.first().length().coerceAtLeast(0L))
        },
        protectedBytes = byPhysicalFile.asSequence()
            .filter { it.key in protectedPhysicalFiles }
            .fold(0L) { total, (_, links) ->
                creationSaturatingBytes(total, links.first().length().coerceAtLeast(0L))
            },
    )
}

internal fun pruneCreationManagedArtifacts(
    roots: List<File>,
    libraryRoot: File,
    protectedPaths: Set<String>,
    recentPaths: Set<String>,
    budgetBytes: Long,
    cleanup: CreationPendingCleanupStore,
) {
    require(budgetBytes >= 0L)
    val protected = protectedPaths.mapNotNull { path ->
        runCatching { File(path).canonicalPath }.getOrNull()
    }.toSet()
    val recent = recentPaths.mapNotNull { path ->
        runCatching { File(path).canonicalPath }.getOrNull()
    }.toSet()
    val library = libraryRoot.canonicalFile
    val uniqueRoots = distinctCreationManagedRoots(roots)
    val candidates = creationPrunableArtifactRoots(uniqueRoots, library)
        .asSequence()
        .flatMap { creationRegularFilesNoFollow(it).asSequence() }
        .distinctBy(File::getAbsolutePath)
        .toList()
    val allManaged = snapshotCreationManagedStorage(uniqueRoots, protected)
    require(allManaged.protectedBytes <= budgetBytes) { CREATION_STORAGE_UNAVAILABLE_ERROR_KEY }
    var total = allManaged.totalBytes
    val physicalGroups = groupCreationPhysicalFiles(
        uniqueRoots.flatMap(::creationRegularFilesNoFollow),
    )
    val remainingLinks = physicalGroups.mapValues { it.value.size }.toMutableMap()
    val physicalKeyByPath = physicalGroups.flatMap { (key, links) ->
        links.map { it.absolutePath to key }
    }.toMap()
    candidates.sortedBy(File::lastModified).forEach { file ->
        if (total <= budgetBytes) return@forEach
        if (file.canonicalPath in protected || file.canonicalPath in recent) return@forEach
        val length = file.length().coerceAtLeast(0L)
        val isolated = cleanup.isolateAndEnqueue(
            listOf(CreationCleanupCandidate.trustedManaged(file.absolutePath)),
        )
        if (file.absolutePath in isolated) {
            val identity = physicalKeyByPath.getValue(file.absolutePath)
            val links = (remainingLinks.getValue(identity) - 1).coerceAtLeast(0)
            remainingLinks[identity] = links
            if (links == 0) total = (total - length).coerceAtLeast(0L)
        }
    }
    cleanup.drain()
}

internal fun creationPrunableArtifactRoots(
    roots: List<File>,
    libraryRoot: File,
): List<File> = roots.filterNot { it.canonicalFile == libraryRoot.canonicalFile }

internal fun creationPhysicalFileIdentity(file: File): String = runCatching {
    Files.readAttributes(
        file.toPath(),
        BasicFileAttributes::class.java,
        LinkOption.NOFOLLOW_LINKS,
    ).fileKey()?.toString()
}.getOrNull()?.let { "key:$it" } ?: "path:${file.canonicalPath}"

internal fun distinctCreationManagedRoots(roots: List<File>): List<File> =
    roots.distinctBy(::creationPhysicalFileIdentity)

private fun groupCreationPhysicalFiles(files: List<File>): Map<String, List<File>> {
    val groups = linkedMapOf<String, MutableList<File>>()
    files.forEach { file ->
        val identity = creationPhysicalFileIdentity(file)
        val key = if (identity.startsWith("key:")) {
            identity
        } else {
            groups.entries.firstOrNull { (_, candidates) ->
                runCatching { Files.isSameFile(candidates.first().toPath(), file.toPath()) }
                    .getOrDefault(false)
            }?.key ?: identity
        }
        groups.getOrPut(key, ::mutableListOf) += file
    }
    return groups
}
