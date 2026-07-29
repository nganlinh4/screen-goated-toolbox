package dev.screengoated.toolbox.mobile.creation

import java.io.File
import java.io.FileInputStream
import java.io.FileOutputStream
import java.nio.file.Files
import java.nio.file.FileVisitResult
import java.nio.file.LinkOption
import java.nio.file.Path
import java.nio.file.SimpleFileVisitor
import java.nio.file.StandardCopyOption
import java.nio.file.attribute.BasicFileAttributes
import java.util.UUID
import org.json.JSONArray
import org.json.JSONObject
import org.json.JSONTokener

internal const val CREATION_HISTORY_INDEX_MAX_BYTES = 4L * 1024 * 1024
internal const val CREATION_JOURNAL_INDEX_MAX_BYTES = 4L * 1024 * 1024

internal fun creationDurableProtectedPaths(filesDir: File): Set<String> =
    creationHistoryProtectedPaths(filesDir) +
        creationJournalProtectedPaths(filesDir) +
        creationIndexProtectedPaths(
            File(filesDir, "creation/pending-cleanup.json"),
            CREATION_CLEANUP_INDEX_MAX_BYTES,
            setOf("artifactPath", "quarantinePath"),
        ) +
        creationIndexProtectedPaths(
            File(filesDir, "creation/state/deliveries.json"),
            CREATION_DELIVERY_INDEX_MAX_BYTES,
            setOf("sealedPath", "targetPath", "pendingPath", "publishedPath"),
        ) +
        creationIndexProtectedPaths(
            File(filesDir, "creation/state/history-renames.json"),
            CREATION_RENAME_INDEX_MAX_BYTES,
            setOf("oldPath", "newPath"),
        )

internal fun creationDurableStateIsReadable(filesDir: File): Boolean = listOf(
    File(filesDir, "creation/history.json") to CREATION_HISTORY_INDEX_MAX_BYTES,
    File(filesDir, "creation/state/accepted-jobs.json") to CREATION_JOURNAL_INDEX_MAX_BYTES,
    File(filesDir, "creation/pending-cleanup.json") to CREATION_CLEANUP_INDEX_MAX_BYTES,
    File(filesDir, "creation/state/deliveries.json") to CREATION_DELIVERY_INDEX_MAX_BYTES,
    File(filesDir, "creation/state/cancellations.json") to CREATION_CANCELLATION_INDEX_MAX_BYTES,
    File(filesDir, "creation/state/owner-closes.json") to CREATION_OWNER_CLOSE_INDEX_MAX_BYTES,
    File(filesDir, "creation/state/job-input-cleanup.json") to
        CREATION_JOB_INPUT_CLEANUP_INDEX_MAX_BYTES,
    File(filesDir, "creation/state/history-renames.json") to CREATION_RENAME_INDEX_MAX_BYTES,
    File(filesDir, "creation/state/uri-grants.json") to CREATION_URI_GRANT_INDEX_MAX_BYTES,
).all { (file, maximumBytes) ->
    if (!file.exists()) {
        true
    } else {
        readCreationIndexTextBounded(file, maximumBytes)
            ?.let { text ->
                runCatching { JSONTokener(text).nextValue() is JSONArray }.getOrDefault(false)
            } == true
    }
}

internal fun creationHistoryProtectedPaths(filesDir: File): Set<String> =
    creationIndexProtectedPaths(
        File(filesDir, "creation/history.json"),
        CREATION_HISTORY_INDEX_MAX_BYTES,
        setOf("outputPath", "sourcePreviewPath", "referencePreviewPaths"),
    )

internal fun creationJournalProtectedPaths(filesDir: File): Set<String> =
    creationIndexProtectedPaths(
        File(filesDir, "creation/state/accepted-jobs.json"),
        CREATION_JOURNAL_INDEX_MAX_BYTES,
        null,
    )

private fun creationIndexProtectedPaths(
    stateFile: File,
    maximumBytes: Long,
    protectedKeys: Set<String>?,
): Set<String> =
    buildSet {
        readCreationIndexTextBounded(stateFile, maximumBytes)
            ?.let { text -> runCatching { JSONTokener(text).nextValue() }.getOrNull() }
            ?.let { collectCreationPaths(it, this, protectedKeys = protectedKeys) }
    }

internal fun readCreationIndexTextBounded(file: File, maximumBytes: Long): String? {
    require(maximumBytes in 1..Int.MAX_VALUE.toLong())
    val path = file.toPath()
    if (isCreationLink(path) ||
        !Files.isRegularFile(path, LinkOption.NOFOLLOW_LINKS)
    ) return null
    return runCatching {
        FileInputStream(file).use { input ->
            val size = input.channel.size()
            if (size !in 0..maximumBytes) return null
            val bytes = ByteArray(size.toInt())
            var offset = 0
            while (offset < bytes.size) {
                val read = input.read(bytes, offset, bytes.size - offset)
                if (read < 0) break
                offset += read
            }
            if (input.read() >= 0) return null
            bytes.decodeToString(endIndex = offset)
        }
    }.getOrNull()
}

internal fun writeCreationIndexTextAtomically(
    file: File,
    text: String,
    maximumBytes: Long,
) {
    val bytes = text.encodeToByteArray()
    require(bytes.size.toLong() <= maximumBytes) { "Creation state is too large" }
    val targetPath = file.toPath().toAbsolutePath().normalize()
    val lock = creationIndexWriteLocks[
        (targetPath.toString().hashCode() and Int.MAX_VALUE) % creationIndexWriteLocks.size
    ]
    synchronized(lock) {
        val directory = requireNotNull(file.parentFile).apply { mkdirs() }
        require(
            Files.isDirectory(directory.toPath(), LinkOption.NOFOLLOW_LINKS) &&
                !isCreationLink(directory.toPath()),
        ) { "Creation state directory is unavailable" }
        val temporary = File(directory, ".${file.name}.tmp-${UUID.randomUUID()}")
        try {
            check(temporary.createNewFile()) { "Could not reserve creation state" }
            FileOutputStream(temporary).use { output ->
                output.write(bytes)
                output.fd.sync()
            }
            runCatching {
                Files.move(
                    temporary.toPath(),
                    targetPath,
                    StandardCopyOption.ATOMIC_MOVE,
                    StandardCopyOption.REPLACE_EXISTING,
                )
            }.getOrElse {
                Files.move(
                    temporary.toPath(),
                    targetPath,
                    StandardCopyOption.REPLACE_EXISTING,
                )
            }
        } finally {
            temporary.delete()
        }
    }
}

private fun collectCreationPaths(
    value: Any?,
    output: MutableSet<String>,
    key: String = "",
    protectedKeys: Set<String>?,
) {
    when (value) {
        is JSONObject -> value.keys().forEach { childKey ->
            collectCreationPaths(value.opt(childKey), output, childKey, protectedKeys)
        }
        is JSONArray -> {
            for (index in 0 until value.length()) {
                collectCreationPaths(value.opt(index), output, key, protectedKeys)
            }
        }
        is String -> if (
            (protectedKeys == null && (key.endsWith("Path") || key.endsWith("Paths"))) ||
            key in protectedKeys.orEmpty()
        ) output += value
    }
}

internal fun creationRegularFilesNoFollow(root: File): List<File> {
    val files = mutableListOf<File>()
    forEachCreationRegularFileNoFollow(root, files::add)
    return files
}

internal fun forEachCreationRegularFileNoFollow(root: File, action: (File) -> Unit) {
    val rootPath = root.toPath().toAbsolutePath().normalize()
    if (!Files.isDirectory(rootPath, LinkOption.NOFOLLOW_LINKS) ||
        isCreationLink(rootPath)
    ) return
    Files.walk(rootPath).use { paths ->
        paths.forEach { path ->
            if (!isCreationLink(path) &&
                Files.isRegularFile(path, LinkOption.NOFOLLOW_LINKS)
            ) {
                action(path.toFile())
            }
        }
    }
}

internal fun deleteCreationFileConfined(root: File, candidate: File): Boolean {
    val rootPath = root.toPath().toAbsolutePath().normalize()
    val candidatePath = candidate.toPath().toAbsolutePath().normalize()
    if (!candidatePath.startsWith(rootPath) ||
        hasCreationLinkBetween(rootPath, candidatePath) ||
        !Files.isRegularFile(candidatePath, LinkOption.NOFOLLOW_LINKS)
    ) return false
    val rootReal = runCatching { rootPath.toRealPath(LinkOption.NOFOLLOW_LINKS) }.getOrNull()
        ?: return false
    val candidateReal = runCatching {
        candidatePath.toRealPath(LinkOption.NOFOLLOW_LINKS)
    }.getOrNull() ?: return false
    if (!candidateReal.startsWith(rootReal)) return false
    return runCatching { Files.deleteIfExists(candidatePath) }.getOrDefault(false)
}

internal data class CreationFileIsolation(
    val original: File,
    val isolated: File,
)

internal fun planCreationFileIsolation(
    root: File,
    candidate: File,
): CreationFileIsolation? {
    val rootPath = root.toPath().toAbsolutePath().normalize()
    val candidatePath = candidate.toPath().toAbsolutePath().normalize()
    if (!candidatePath.startsWith(rootPath) ||
        candidatePath == rootPath ||
        hasCreationLinkBetween(rootPath, candidatePath) ||
        !Files.isRegularFile(candidatePath, LinkOption.NOFOLLOW_LINKS)
    ) return null
    val isolatedPath = candidatePath.resolveSibling(
        ".${candidatePath.fileName}.cleanup-${UUID.randomUUID()}",
    )
    return CreationFileIsolation(candidatePath.toFile(), isolatedPath.toFile())
}

internal fun isolateCreationFileConfined(
    root: File,
    isolation: CreationFileIsolation,
): Boolean {
    val rootPath = root.toPath().toAbsolutePath().normalize()
    val originalPath = isolation.original.toPath().toAbsolutePath().normalize()
    val isolatedPath = isolation.isolated.toPath().toAbsolutePath().normalize()
    if (!originalPath.startsWith(rootPath) ||
        !isolatedPath.startsWith(rootPath) ||
        originalPath.parent != isolatedPath.parent ||
        Files.exists(isolatedPath, LinkOption.NOFOLLOW_LINKS) ||
        hasCreationLinkBetween(rootPath, originalPath) ||
        !Files.isRegularFile(originalPath, LinkOption.NOFOLLOW_LINKS)
    ) return false
    return runCatching {
        Files.move(originalPath, isolatedPath, StandardCopyOption.ATOMIC_MOVE)
        true
    }.getOrDefault(false)
}

internal fun sealCreationStagingFile(root: File, candidate: File): File? {
    val rootPath = root.toPath().toAbsolutePath().normalize()
    val candidatePath = candidate.toPath().toAbsolutePath().normalize()
    if (candidatePath.parent != rootPath ||
        !isCreationRegularFileConfined(root, candidate)
    ) return null
    // The accepted-job journal already owns this exact reserved path. Keeping it
    // stable closes the crash cut between validation and the delivery receipt.
    return candidatePath.toFile()
}

internal fun isCreationRegularFileConfined(root: File, candidate: File): Boolean {
    val rootPath = root.toPath().toAbsolutePath().normalize()
    val candidatePath = candidate.toPath().toAbsolutePath().normalize()
    return candidatePath.startsWith(rootPath) &&
        candidatePath != rootPath &&
        !isCreationLink(rootPath) &&
        !hasCreationLinkBetween(rootPath, candidatePath) &&
        Files.isRegularFile(candidatePath, LinkOption.NOFOLLOW_LINKS)
}

internal fun restoreCreationFileConfined(
    root: File,
    isolation: CreationFileIsolation,
): Boolean {
    val rootPath = root.toPath().toAbsolutePath().normalize()
    val originalPath = isolation.original.toPath().toAbsolutePath().normalize()
    val isolatedPath = isolation.isolated.toPath().toAbsolutePath().normalize()
    if (!originalPath.startsWith(rootPath) ||
        !isolatedPath.startsWith(rootPath) ||
        originalPath.parent != isolatedPath.parent ||
        Files.exists(originalPath, LinkOption.NOFOLLOW_LINKS) ||
        hasCreationLinkBetween(rootPath, isolatedPath) ||
        !Files.isRegularFile(isolatedPath, LinkOption.NOFOLLOW_LINKS)
    ) return false
    return runCatching {
        Files.move(isolatedPath, originalPath, StandardCopyOption.ATOMIC_MOVE)
        true
    }.getOrDefault(false)
}

internal fun planCreationRelinquishedFile(
    root: File,
    isolation: CreationFileIsolation,
): File? {
    val rootPath = root.toPath().toAbsolutePath().normalize()
    val isolatedPath = isolation.isolated.toPath().toAbsolutePath().normalize()
    if (!isolatedPath.startsWith(rootPath) ||
        hasCreationLinkBetween(rootPath, isolatedPath) ||
        !Files.isRegularFile(isolatedPath, LinkOption.NOFOLLOW_LINKS)
    ) return null
    val destinationDirectory = rootPath.parent.resolve("relinquished").normalize()
    if (destinationDirectory.startsWith(rootPath)) return null
    return runCatching {
        Files.createDirectories(destinationDirectory)
        if (isCreationLink(destinationDirectory)) return null
        destinationDirectory.resolve(
            "${UUID.randomUUID()}-${isolation.original.name}",
        ).toFile()
    }.getOrNull()
}

internal fun relinquishCreationFileConfined(
    root: File,
    isolation: CreationFileIsolation,
    destination: File,
): Boolean {
    val rootPath = root.toPath().toAbsolutePath().normalize()
    val isolatedPath = isolation.isolated.toPath().toAbsolutePath().normalize()
    val destinationPath = destination.toPath().toAbsolutePath().normalize()
    val destinationDirectory = rootPath.parent.resolve("relinquished").normalize()
    if (!isolatedPath.startsWith(rootPath) ||
        destinationPath.parent != destinationDirectory ||
        Files.exists(destinationPath, LinkOption.NOFOLLOW_LINKS) ||
        hasCreationLinkBetween(rootPath, isolatedPath) ||
        isCreationLink(destinationDirectory) ||
        !Files.isRegularFile(isolatedPath, LinkOption.NOFOLLOW_LINKS)
    ) return false
    return runCatching {
        Files.move(isolatedPath, destinationPath, StandardCopyOption.ATOMIC_MOVE)
        true
    }.getOrDefault(false)
}

internal fun deleteCreationTreeNoFollow(root: File, target: File): Boolean {
    val rootPath = root.toPath().toAbsolutePath().normalize()
    val targetPath = target.toPath().toAbsolutePath().normalize()
    if (!targetPath.startsWith(rootPath) || targetPath == rootPath) return false
    if (isCreationLink(targetPath)) {
        return deleteCreationLinkConfined(rootPath, targetPath)
    }
    if (hasCreationLinkBetween(rootPath, targetPath) ||
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
                    complete = if (isCreationLink(file)) {
                        deleteCreationLinkConfined(rootPath, file) && complete
                    } else {
                        deleteCreationFileConfined(root, file.toFile()) && complete
                    }
                    return FileVisitResult.CONTINUE
                }

                override fun postVisitDirectory(
                    dir: Path,
                    error: java.io.IOException?,
                ): FileVisitResult {
                    complete = error == null &&
                        runCatching { Files.deleteIfExists(dir) }.getOrDefault(false) &&
                        complete
                    return FileVisitResult.CONTINUE
                }
            },
        )
    }.onFailure {
        complete = false
    }
    return complete
}

internal fun creationChildDirectoriesNoFollow(root: File): List<File> {
    val path = root.toPath().toAbsolutePath().normalize()
    if (!Files.isDirectory(path, LinkOption.NOFOLLOW_LINKS) || isCreationLink(path)) {
        return emptyList()
    }
    return Files.newDirectoryStream(path).use { children ->
        children.iterator().asSequence()
            .take(MAXIMUM_CREATION_DIRECTORY_CHILDREN)
            .filter {
                Files.isDirectory(it, LinkOption.NOFOLLOW_LINKS) && !isCreationLink(it)
            }
            .map(Path::toFile)
            .toList()
    }
}

private fun deleteCreationLinkConfined(root: Path, candidate: Path): Boolean {
    if (!candidate.startsWith(root) ||
        candidate == root ||
        !isCreationLink(candidate) ||
        hasCreationLinkBetween(root, candidate.parent)
    ) return false
    val rootReal = runCatching { root.toRealPath(LinkOption.NOFOLLOW_LINKS) }.getOrNull()
        ?: return false
    val parentReal = runCatching {
        candidate.parent.toRealPath(LinkOption.NOFOLLOW_LINKS)
    }.getOrNull() ?: return false
    if (!parentReal.startsWith(rootReal)) return false
    return runCatching { Files.deleteIfExists(candidate) }.getOrDefault(false)
}

private fun hasCreationLinkBetween(root: Path, candidate: Path): Boolean {
    var current: Path? = candidate
    while (current != null && current.startsWith(root)) {
        if (isCreationLink(current)) return true
        if (current == root) return false
        current = current.parent
    }
    return true
}

internal fun isCreationLink(path: Path): Boolean =
    Files.isSymbolicLink(path) ||
        runCatching {
            Files.getAttribute(path, "dos:reparsePoint", LinkOption.NOFOLLOW_LINKS) == true
        }.getOrDefault(false)

private const val MAXIMUM_CREATION_DIRECTORY_CHILDREN = 256
private val creationIndexWriteLocks = Array(32) { Any() }
