package dev.screengoated.toolbox.mobile.downloader

import android.content.Context
import android.system.Os
import java.io.File
import java.io.FileOutputStream
import java.io.RandomAccessFile
import java.nio.charset.StandardCharsets
import java.nio.file.FileVisitResult
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.SimpleFileVisitor
import java.nio.file.StandardCopyOption
import java.nio.file.attribute.BasicFileAttributes
import java.security.MessageDigest
import java.util.UUID
import java.util.zip.ZipInputStream
import kotlinx.coroutines.ensureActive
import okhttp3.OkHttpClient
import okhttp3.Request
import kotlin.coroutines.coroutineContext

internal data class DownloaderInstallProgress(
    val role: DownloaderArtifactRole,
    val fraction: Float,
    val extracting: Boolean,
)

internal class DownloaderRuntimeInstaller(
    context: Context,
    private val delivery: DownloaderRuntimeDelivery,
    private val httpClient: OkHttpClient,
) {
    private val context = context.applicationContext
    private val runtimeRoot = File(this.context.noBackupFilesDir, "downloader-runtime")
    val versionDirectory = File(runtimeRoot, delivery.version)
    val ytdlpFile = File(versionDirectory, "bin/yt-dlp")
    val pythonDirectory = File(versionDirectory, "packages/python")
    val ffmpegDirectory = File(versionDirectory, "packages/ffmpeg")
    private val identityFile = File(versionDirectory, ".identity")

    fun isInstalled(): Boolean {
        if (identityFile.readTextOrNull() != delivery.identity) return false
        val ytdlp = delivery.artifact(DownloaderArtifactRole.YT_DLP)
        if (!validFile(ytdlpFile, ytdlp.sizeBytes, ytdlp.sha256)) return false
        return delivery.artifacts
            .filter { it.role != DownloaderArtifactRole.YT_DLP }
            .all { artifact ->
                val root = installDirectory(artifact.role)
                root.isDirectory && artifact.requiredPaths.all { path ->
                    safeTarget(root, path).exists()
                }
            }
    }

    suspend fun install(onProgress: (DownloaderInstallProgress) -> Unit) {
        if (isInstalled()) return
        runtimeRoot.mkdirs()
        val staging = File(runtimeRoot, ".install-${UUID.randomUUID()}")
        safeDeleteTree(runtimeRoot, staging)
        check(staging.mkdirs()) { "Could not create downloader staging directory" }
        try {
            delivery.artifacts.forEach { artifact ->
                coroutineContext.ensureActive()
                val partial = partialFile(artifact)
                try {
                    download(artifact, partial) { fraction ->
                        onProgress(DownloaderInstallProgress(artifact.role, fraction, false))
                    }
                    when (artifact.role) {
                        DownloaderArtifactRole.YT_DLP -> {
                            val target = File(staging, "bin/yt-dlp")
                            target.parentFile?.mkdirs()
                            Files.move(
                                partial.toPath(),
                                target.toPath(),
                                StandardCopyOption.REPLACE_EXISTING,
                            )
                            check(target.setReadOnly()) { "Could not lock yt-dlp" }
                        }
                        DownloaderArtifactRole.PYTHON,
                        DownloaderArtifactRole.FFMPEG -> {
                            onProgress(DownloaderInstallProgress(artifact.role, 1f, true))
                            val target = when (artifact.role) {
                                DownloaderArtifactRole.PYTHON -> File(staging, "packages/python")
                                DownloaderArtifactRole.FFMPEG -> File(staging, "packages/ffmpeg")
                            }
                            extractArchive(partial, target, artifact)
                        }
                    }
                } finally {
                    partial.delete()
                }
            }
            delivery.artifacts
                .filter { it.role != DownloaderArtifactRole.YT_DLP }
                .forEach { artifact ->
                    val root = when (artifact.role) {
                        DownloaderArtifactRole.PYTHON -> File(staging, "packages/python")
                        DownloaderArtifactRole.FFMPEG -> File(staging, "packages/ffmpeg")
                        else -> error("Unsupported downloader archive")
                    }
                    check(artifact.requiredPaths.all { safeTarget(root, it).exists() }) {
                        "${artifact.role.wireName} archive is missing required files"
                    }
                }
            File(staging, ".identity").writeText(delivery.identity)
            safeDeleteTree(runtimeRoot, versionDirectory)
            moveDirectory(staging.toPath(), versionDirectory.toPath())
            check(isInstalled()) { "Installed downloader runtime failed validation" }
            cleanupOtherVersions()
            removeLegacyRuntime()
        } finally {
            safeDeleteTree(runtimeRoot, staging)
            delivery.artifacts.forEach { partialFile(it).delete() }
        }
    }

    fun remove(): Boolean {
        delivery.artifacts.forEach { partialFile(it).delete() }
        safeDeleteTree(runtimeRoot, runtimeRoot)
        removeLegacyRuntime()
        return !runtimeRoot.exists() && !legacyRoots().any(File::exists)
    }

    fun installedBytes(): Long = sequenceOf(runtimeRoot, *legacyRoots().toTypedArray())
        .filter(File::exists)
        .sumOf(::treeBytesNoFollow)

    fun componentBytes(role: DownloaderArtifactRole): Long = when (role) {
        DownloaderArtifactRole.YT_DLP -> ytdlpFile.takeIf(File::isFile)?.length() ?: 0L
        DownloaderArtifactRole.PYTHON ->
            pythonDirectory.takeIf(File::exists)?.let(::treeBytesNoFollow) ?: 0L
        DownloaderArtifactRole.FFMPEG ->
            ffmpegDirectory.takeIf(File::exists)?.let(::treeBytesNoFollow) ?: 0L
    }

    private suspend fun download(
        artifact: DownloaderRuntimeArtifact,
        partial: File,
        onProgress: (Float) -> Unit,
    ) {
        partial.parentFile?.mkdirs()
        partial.delete()
        val digest = MessageDigest.getInstance("SHA-256")
        val request = Request.Builder().url(artifact.downloadUrl).build()
        httpClient.newCall(request).execute().use { response ->
            check(response.isSuccessful) { "Downloader runtime HTTP ${response.code}" }
            val declaredBytes = response.body.contentLength()
            check(declaredBytes < 0L || declaredBytes == artifact.sizeBytes) {
                "${artifact.asset} response has an unexpected size"
            }
            var written = 0L
            FileOutputStream(partial).use { output ->
                response.body.byteStream().use { input ->
                    val buffer = ByteArray(BUFFER_BYTES)
                    while (true) {
                        coroutineContext.ensureActive()
                        val read = input.read(buffer)
                        if (read < 0) break
                        written += read
                        check(written <= artifact.sizeBytes) {
                            "${artifact.asset} download is oversized"
                        }
                        output.write(buffer, 0, read)
                        digest.update(buffer, 0, read)
                        onProgress(written.toFloat() / artifact.sizeBytes)
                    }
                    output.fd.sync()
                }
            }
            check(written == artifact.sizeBytes) { "${artifact.asset} download is incomplete" }
            check(digest.hex() == artifact.sha256) { "${artifact.asset} failed SHA-256 validation" }
        }
    }

    private suspend fun extractArchive(
        archive: File,
        targetRoot: File,
        artifact: DownloaderRuntimeArtifact,
    ) {
        val symlinks = unixSymlinkEntries(archive, requireNotNull(artifact.entryCount))
        val seen = mutableSetOf<String>()
        var entryCount = 0
        var uncompressedBytes = 0L
        targetRoot.mkdirs()
        ZipInputStream(archive.inputStream().buffered()).use { zip ->
            while (true) {
                coroutineContext.ensureActive()
                val entry = zip.nextEntry ?: break
                entryCount += 1
                check(entryCount <= requireNotNull(artifact.entryCount)) {
                    "${artifact.asset} contains too many entries"
                }
                check(isSafeRelativePath(entry.name.removeSuffix("/"))) {
                    "${artifact.asset} contains an unsafe path"
                }
                check(seen.add(entry.name)) { "${artifact.asset} repeats an entry" }
                val target = safeTarget(targetRoot, entry.name.removeSuffix("/"))
                when {
                    entry.isDirectory -> check(target.mkdirs() || target.isDirectory) {
                        "Could not create downloader archive directory"
                    }
                    entry.name in symlinks -> {
                        target.parentFile?.mkdirs()
                        val linkBytes = zip.readBytesBounded(MAX_SYMLINK_BYTES)
                        uncompressedBytes += linkBytes.size
                        val link = linkBytes.toString(StandardCharsets.UTF_8)
                        check(isSafeSymlink(targetRoot, target, link)) {
                            "${artifact.asset} contains an unsafe symlink"
                        }
                        Os.symlink(link, target.absolutePath)
                    }
                    else -> {
                        target.parentFile?.mkdirs()
                        FileOutputStream(target).use { output ->
                            val buffer = ByteArray(BUFFER_BYTES)
                            while (true) {
                                coroutineContext.ensureActive()
                                val read = zip.read(buffer)
                                if (read < 0) break
                                uncompressedBytes += read
                                check(uncompressedBytes <= requireNotNull(artifact.uncompressedBytes)) {
                                    "${artifact.asset} expands beyond its contract"
                                }
                                output.write(buffer, 0, read)
                            }
                            output.fd.sync()
                        }
                    }
                }
                zip.closeEntry()
            }
        }
        check(entryCount == artifact.entryCount) { "${artifact.asset} entry count differs" }
        check(uncompressedBytes == artifact.uncompressedBytes) {
            "${artifact.asset} expanded byte count differs"
        }
    }

    private fun cleanupOtherVersions() {
        runtimeRoot.listFiles()?.filter { it != versionDirectory }?.forEach {
            safeDeleteTree(runtimeRoot, it)
        }
    }

    private fun removeLegacyRuntime() {
        legacyRoots().forEach { root -> safeDeleteTree(root.parentFile ?: root, root) }
        listOf("youtubedl-android", "com.yausername.youtubedl_android").forEach { name ->
            context.getSharedPreferences(name, Context.MODE_PRIVATE).edit().clear().apply()
        }
    }

    private fun legacyRoots(): List<File> = listOf(
        File(context.noBackupFilesDir, "youtubedl-android"),
        File(context.filesDir, "youtubedl-android"),
        File(context.dataDir, "app_ytdl_native"),
    )

    private fun installDirectory(role: DownloaderArtifactRole): File = when (role) {
        DownloaderArtifactRole.PYTHON -> pythonDirectory
        DownloaderArtifactRole.FFMPEG -> ffmpegDirectory
        DownloaderArtifactRole.YT_DLP -> ytdlpFile.parentFile ?: versionDirectory
    }

    private fun partialFile(artifact: DownloaderRuntimeArtifact) =
        File(context.cacheDir, "${artifact.asset}.part")

    private companion object {
        const val BUFFER_BYTES = 128 * 1024
        const val MAX_SYMLINK_BYTES = 4096
    }
}

private fun unixSymlinkEntries(archive: File, expectedEntries: Int): Set<String> {
    RandomAccessFile(archive, "r").use { file ->
        val tailSize = minOf(file.length(), 65_557L).toInt()
        val tail = ByteArray(tailSize)
        file.seek(file.length() - tailSize)
        file.readFully(tail)
        val eocd = (tailSize - 22 downTo 0).firstOrNull {
            tail.leInt(it) == 0x06054b50
        } ?: error("ZIP central directory is missing")
        val entries = tail.leShort(eocd + 10)
        check(entries == expectedEntries) { "ZIP central entry count differs" }
        val centralOffset = tail.leUnsignedInt(eocd + 16)
        check(centralOffset < file.length()) { "ZIP central directory offset is invalid" }
        file.seek(centralOffset)
        return buildSet {
            repeat(entries) {
                val header = ByteArray(46)
                file.readFully(header)
                check(header.leInt(0) == 0x02014b50) { "Invalid ZIP central entry" }
                val nameLength = header.leShort(28)
                val extraLength = header.leShort(30)
                val commentLength = header.leShort(32)
                val unixMode = (header.leUnsignedInt(38) ushr 16).toInt()
                val nameBytes = ByteArray(nameLength)
                file.readFully(nameBytes)
                val name = nameBytes.toString(StandardCharsets.UTF_8)
                check(isSafeRelativePath(name.removeSuffix("/"))) { "Unsafe ZIP central path" }
                if (unixMode and 0xF000 == 0xA000) add(name)
                file.seek(file.filePointer + extraLength + commentLength)
            }
        }
    }
}

private fun ByteArray.leShort(offset: Int): Int =
    (this[offset].toInt() and 0xff) or ((this[offset + 1].toInt() and 0xff) shl 8)

private fun ByteArray.leInt(offset: Int): Int =
    leUnsignedInt(offset).toInt()

private fun ByteArray.leUnsignedInt(offset: Int): Long =
    (this[offset].toLong() and 0xff) or
        ((this[offset + 1].toLong() and 0xff) shl 8) or
        ((this[offset + 2].toLong() and 0xff) shl 16) or
        ((this[offset + 3].toLong() and 0xff) shl 24)

private fun ZipInputStream.readBytesBounded(maxBytes: Int): ByteArray {
    val output = ArrayList<Byte>()
    while (true) {
        val value = read()
        if (value < 0) break
        check(output.size < maxBytes) { "ZIP symlink target is too long" }
        output += value.toByte()
    }
    return output.toByteArray()
}

private fun isSafeSymlink(root: File, linkFile: File, link: String): Boolean {
    if (link.isBlank() || link.startsWith('/') || link.startsWith('\\') || ':' in link) return false
    val rootPath = root.toPath().toAbsolutePath().normalize()
    val parent = linkFile.parentFile?.toPath()?.toAbsolutePath()?.normalize() ?: return false
    return parent.resolve(link).normalize().startsWith(rootPath)
}

private fun safeTarget(root: File, relativePath: String): File {
    val rootPath = root.toPath().toAbsolutePath().normalize()
    val target = rootPath.resolve(relativePath).normalize()
    require(target.startsWith(rootPath) && target != rootPath) { "Path escapes downloader root" }
    return target.toFile()
}

private fun safeDeleteTree(root: File, target: File) {
    if (!target.exists() && !Files.isSymbolicLink(target.toPath())) return
    val rootPath = root.toPath().toAbsolutePath().normalize()
    val targetPath = target.toPath().toAbsolutePath().normalize()
    require(targetPath == rootPath || targetPath.startsWith(rootPath)) {
        "Deletion escapes downloader root"
    }
    Files.walkFileTree(targetPath, object : SimpleFileVisitor<Path>() {
        override fun visitFile(file: Path, attrs: BasicFileAttributes): FileVisitResult {
            Files.deleteIfExists(file)
            return FileVisitResult.CONTINUE
        }

        override fun postVisitDirectory(dir: Path, error: java.io.IOException?): FileVisitResult {
            if (error != null) throw error
            Files.deleteIfExists(dir)
            return FileVisitResult.CONTINUE
        }
    })
}

private fun moveDirectory(source: Path, target: Path) {
    runCatching {
        Files.move(source, target, StandardCopyOption.ATOMIC_MOVE)
    }.getOrElse {
        Files.move(source, target)
    }
}

private fun validFile(file: File, sizeBytes: Long, sha256: String): Boolean {
    if (!file.isFile || file.length() != sizeBytes) return false
    val digest = MessageDigest.getInstance("SHA-256")
    file.inputStream().use { input ->
        val buffer = ByteArray(128 * 1024)
        while (true) {
            val read = input.read(buffer)
            if (read < 0) break
            digest.update(buffer, 0, read)
        }
    }
    return digest.hex() == sha256
}

private fun MessageDigest.hex(): String = digest().joinToString("") { "%02x".format(it) }

private fun File.readTextOrNull(): String? = runCatching { readText() }.getOrNull()

private fun treeBytesNoFollow(root: File): Long = Files.walk(root.toPath()).use { paths ->
    paths.filter { Files.isRegularFile(it, java.nio.file.LinkOption.NOFOLLOW_LINKS) }
        .mapToLong { Files.readAttributes(it, BasicFileAttributes::class.java).size() }
        .sum()
}
