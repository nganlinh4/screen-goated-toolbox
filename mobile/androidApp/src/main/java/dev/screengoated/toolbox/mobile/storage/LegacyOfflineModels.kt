package dev.screengoated.toolbox.mobile.storage

import dev.screengoated.toolbox.mobile.creation.deleteCreationTreeNoFollow
import dev.screengoated.toolbox.mobile.creation.isCreationLink
import java.io.File
import java.io.IOException
import java.nio.file.FileVisitResult
import java.nio.file.Files
import java.nio.file.LinkOption
import java.nio.file.Path
import java.nio.file.SimpleFileVisitor
import java.nio.file.attribute.BasicFileAttributes

/**
 * Files that the removed on-device speech engines (Moonshine, Zipformer and their native
 * runtimes) left in app storage. Nothing is deleted until the user asks; removal touches only
 * these exact folders, never follows links, and stays inside the app's own storage roots.
 */
internal class LegacyOfflineModels(
    private val filesDir: File,
    private val cacheDir: File,
    private val deletePreferences: (String) -> Unit,
) {
    private class Target(val root: File, val relativePath: String) {
        val folder: File get() = File(root, relativePath)
    }

    private val targets = listOf(
        Target(filesDir, "models/moonshine"),
        Target(filesDir, "native-libs"),
        Target(cacheDir, "moonshine-model-bundles"),
    )

    /** Total bytes held by the legacy folders, or null when none exists. */
    fun presentBytes(): Long? {
        val present = targets.filter { Files.exists(it.folder.toPath(), LinkOption.NOFOLLOW_LINKS) }
        if (present.isEmpty()) return null
        return present.sumOf { regularFileBytesNoFollow(it.folder.toPath()) }
    }

    /** Deletes every legacy folder and the engines' bookkeeping; true when nothing remains. */
    fun remove(): Boolean {
        var complete = true
        for (target in targets) {
            if (!Files.exists(target.folder.toPath(), LinkOption.NOFOLLOW_LINKS)) continue
            complete = deleteCreationTreeNoFollow(target.root, target.folder) && complete
        }
        removeIfEmptyDirectory(File(filesDir, "models"))
        LEGACY_PREFERENCES.forEach(deletePreferences)
        return complete && presentBytes() == null
    }

    private fun removeIfEmptyDirectory(directory: File) {
        val path = directory.toPath()
        if (!Files.isDirectory(path, LinkOption.NOFOLLOW_LINKS) || isCreationLink(path)) return
        val empty = Files.newDirectoryStream(path).use { !it.iterator().hasNext() }
        if (empty) runCatching { Files.deleteIfExists(path) }
    }

    private fun regularFileBytesNoFollow(root: Path): Long {
        if (isCreationLink(root) || !Files.isDirectory(root, LinkOption.NOFOLLOW_LINKS)) return 0L
        var total = 0L
        runCatching {
            Files.walkFileTree(
                root,
                object : SimpleFileVisitor<Path>() {
                    override fun visitFile(file: Path, attrs: BasicFileAttributes): FileVisitResult {
                        if (attrs.isRegularFile) total += attrs.size()
                        return FileVisitResult.CONTINUE
                    }

                    override fun visitFileFailed(file: Path, exc: IOException): FileVisitResult =
                        FileVisitResult.CONTINUE
                },
            )
        }
        return total
    }

    private companion object {
        /** SharedPreferences files that tracked pending removals for the removed engines. */
        val LEGACY_PREFERENCES = listOf(
            "downloaded_model_lifecycle",
            "downloaded_native_runtime_lifecycle",
        )
    }
}
