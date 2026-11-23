package dev.screengoated.toolbox.mobile.downloader

import java.io.File
import java.io.FileOutputStream
import java.nio.file.Files
import java.nio.file.LinkOption.NOFOLLOW_LINKS
import java.nio.file.StandardCopyOption.ATOMIC_MOVE
import java.util.concurrent.atomic.AtomicBoolean
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import org.json.JSONObject

internal data class InstalledYtDlp(val version: YtDlpVersion, val executable: File)

internal class YtDlpLease(val tool: InstalledYtDlp, private val onClose: () -> Unit) : AutoCloseable {
    private val closed = AtomicBoolean(false)

    override fun close() {
        if (closed.compareAndSet(false, true)) onClose()
    }
}

/** Publishes one verified executable without moving or reinstalling Android runtimes. */
internal class YtDlpInstallStore(
    private val root: File,
    private val bootstrap: InstalledYtDlp,
    private val nativeIdentity: String,
) {
    private val lock = Any()
    private val leases = mutableMapOf<File, Int>()
    private var stagingRecovered = false

    fun current(): InstalledYtDlp = synchronized(lock) { currentLocked() }

    fun acquire(): YtDlpLease = synchronized(lock) {
        val tool = currentLocked()
        val executable = tool.executable
        leases[executable] = leases.getOrDefault(executable, 0) + 1
        YtDlpLease(tool) {
            synchronized(lock) {
                val remaining = leases.getValue(executable) - 1
                if (remaining == 0) leases.remove(executable) else leases[executable] = remaining
                prune()
            }
        }
    }

    suspend fun install(
        asset: VerifiedYtDlpAsset,
        download: suspend (File) -> Unit,
        probe: (File, YtDlpVersion) -> Unit,
    ): Boolean {
        if (asset.version <= current().version) return false
        ensureRoot()
        val staging = Files.createTempDirectory(root.toPath(), ".candidate-").toFile()
        try {
            val executable = staging.resolve(EXECUTABLE)
            download(executable)
            check(validFile(executable, asset.size, asset.sha256)) { "Invalid yt-dlp candidate" }
            check(executable.setReadOnly()) { "Could not protect the yt-dlp candidate" }
            probe(executable, asset.version)
            currentCoroutineContext().ensureActive()
            val receipt = JSONObject()
                .put("version", asset.version.tag)
                .put("size", asset.size)
                .put("sha256", asset.sha256)
                .put("nativeIdentity", nativeIdentity)
            FileOutputStream(staging.resolve(RECEIPT)).use { output ->
                output.write(receipt.toString().encodeToByteArray())
                output.fd.sync()
            }
            return synchronized(lock) {
                if (asset.version <= currentLocked().version) return@synchronized false
                val target = root.resolve("${asset.version.tag}-${asset.sha256.take(16)}")
                check(leases[target.resolve(EXECUTABLE)] == null) { "The yt-dlp candidate is in use" }
                safeDeleteTree(root, target)
                Files.move(staging.toPath(), target.toPath(), ATOMIC_MOVE)
                prune()
                true
            }
        } finally {
            val uninstalled = staging.resolve(EXECUTABLE)
            if (Files.isRegularFile(uninstalled.toPath(), NOFOLLOW_LINKS)) uninstalled.setWritable(true)
            safeDeleteTree(root, staging)
        }
    }

    fun installedBytes(): Long = synchronized(lock) {
        if (Files.isDirectory(root.toPath(), NOFOLLOW_LINKS)) treeBytesNoFollow(root) else 0
    }

    fun remove(): Boolean = synchronized(lock) {
        check(leases.isEmpty()) { "The downloader is still using yt-dlp" }
        safeDeleteTree(root, root)
        !Files.exists(root.toPath(), NOFOLLOW_LINKS)
    }

    private fun currentLocked(): InstalledYtDlp =
        alternatives().firstOrNull { it.version > bootstrap.version } ?: bootstrap

    private fun alternatives(): List<InstalledYtDlp> {
        if (!Files.isDirectory(root.toPath(), NOFOLLOW_LINKS)) return emptyList()
        return root.listFiles().orEmpty().mapNotNull(::load).sortedByDescending(InstalledYtDlp::version)
    }

    private fun load(directory: File): InstalledYtDlp? = runCatching {
        if (!CANDIDATE_DIRECTORY.matches(directory.name) ||
            !Files.isDirectory(directory.toPath(), NOFOLLOW_LINKS)
        ) return null
        val receipt = directory.resolve(RECEIPT)
        if (!Files.isRegularFile(receipt.toPath(), NOFOLLOW_LINKS) || receipt.length() !in 1..4096) return null
        val data = JSONObject(receipt.readText())
        if (data.getString("nativeIdentity") != nativeIdentity) return null
        val version = YtDlpVersion.parse(data.getString("version")) ?: return null
        val sha256 = data.getString("sha256")
        val size = data.getLong("size")
        if (!SHA_256.matches(sha256) || size !in 1..MAX_YT_DLP_BYTES ||
            directory.name != "${version.tag}-${sha256.take(16)}"
        ) return null
        val executable = directory.resolve(EXECUTABLE)
        if (!validFile(executable, size, sha256)) return null
        InstalledYtDlp(version, executable)
    }.getOrNull()

    private fun ensureRoot() = synchronized(lock) {
        if (!Files.exists(root.toPath(), NOFOLLOW_LINKS)) root.mkdirs()
        check(Files.isDirectory(root.toPath(), NOFOLLOW_LINKS)) { "The yt-dlp update directory is invalid" }
        if (!stagingRecovered) {
            root.listFiles().orEmpty().filter { it.name.startsWith(".candidate-") }.forEach {
                safeDeleteTree(root, it)
            }
            stagingRecovered = true
        }
    }

    private fun prune() {
        val retained = alternatives().take(2).map { it.executable }.toSet()
        root.listFiles().orEmpty().filter { CANDIDATE_DIRECTORY.matches(it.name) }.forEach { directory ->
            val executable = directory.resolve(EXECUTABLE)
            if (executable !in retained && executable !in leases) {
                runCatching { safeDeleteTree(root, directory) }
            }
        }
    }

    private companion object {
        const val EXECUTABLE = "yt-dlp"
        const val RECEIPT = "verified.json"
        val CANDIDATE_DIRECTORY = Regex("""[0-9.]+-[a-f0-9]{16}""")
        val SHA_256 = Regex("""[a-f0-9]{64}""")
    }
}

internal fun DownloaderRuntimeDelivery.ytDlpStore(root: File, bootstrapFile: File): YtDlpInstallStore {
    val url = artifact(DownloaderArtifactRole.YT_DLP).downloadUrl
    val version = requireNotNull(YtDlpVersion.parse(url.substringAfter("/download/").substringBefore('/')))
    val nativeIdentity = listOf(DownloaderArtifactRole.PYTHON, DownloaderArtifactRole.FFMPEG)
        .joinToString(":") { artifact(it).sha256 }
    return YtDlpInstallStore(root, InstalledYtDlp(version, bootstrapFile), nativeIdentity)
}
