package dev.screengoated.toolbox.mobile.creation.runtime

import android.content.Context
import dalvik.system.DexClassLoader
import dev.screengoated.toolbox.mobile.creation.creationChildDirectoriesNoFollow
import dev.screengoated.toolbox.mobile.creation.deleteCreationTreeNoFollow
import java.io.File
import java.io.FileOutputStream
import java.security.MessageDigest
import java.util.UUID
import java.util.zip.ZipInputStream
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import okhttp3.OkHttpClient
import okhttp3.Request

internal class CreationRuntimeProvider(private val context: Context) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val httpClient = OkHttpClient()
    private val delivery: CreationRuntimeDelivery? by lazy {
        loadCreationRuntimeDelivery(context)
    }
    private val mutableStatus = MutableStateFlow(computeStatus())
    private var installJob: Job? = null
    private var removalJob: Job? = null
    @Volatile private var loadedFactory: CreationRuntimeFactory? = null

    val status: StateFlow<CreationRuntimeStatus> = mutableStatus.asStateFlow()

    fun startInstall() {
        if (mutableStatus.value is CreationRuntimeStatus.RemovalPending ||
            factory() != null || installJob?.isActive == true || removalJob?.isActive == true
        ) return
        if (delivery == null) {
            mutableStatus.value = CreationRuntimeStatus.Failed(CREATION_RUNTIME_INSTALL_FAILURE)
            return
        }
        installJob = scope.launch {
            mutableStatus.value = CreationRuntimeStatus.Downloading(0f)
            try {
                installBundle()
                val factory = loadFactory() ?: error("Creation runtime could not be loaded")
                loadedFactory = factory
                mutableStatus.value = CreationRuntimeStatus.Ready(installedBytes())
            } catch (cancelled: CancellationException) {
                throw cancelled
            } catch (_: Throwable) {
                mutableStatus.value = CreationRuntimeStatus.Failed(CREATION_RUNTIME_INSTALL_FAILURE)
            } finally {
                installJob = null
            }
        }
    }

    fun factory(): CreationRuntimeFactory? {
        if (mutableStatus.value is CreationRuntimeStatus.RemovalPending) return null
        loadedFactory?.let { return it }
        if (!installedFilesAreValid()) return null
        return loadFactory()?.also {
            loadedFactory = it
            mutableStatus.value = CreationRuntimeStatus.Ready(installedBytes())
        }
    }

    fun delete() {
        if (removalJob?.isActive == true) return
        val activeInstall = installJob
        activeInstall?.cancel()
        loadedFactory = null
        mutableStatus.value = CreationRuntimeStatus.RemovalPending(
            "Removal pending while Creation workers stop.",
        )
        removalJob = scope.launch {
            activeInstall?.cancelAndJoin()
            try {
                deleteRuntimeTree(context.filesDir, runtimeRoot())
                deleteRuntimeTree(context.codeCacheDir, optimizedRoot())
                check(!bundlePartial().exists() || bundlePartial().delete()) {
                    "Creation runtime partial download could not be removed"
                }
                mutableStatus.value = computeStatus()
            } catch (error: Throwable) {
                mutableStatus.value = CreationRuntimeStatus.RemovalPending(
                    error.message ?: "Creation runtime removal failed. Try again.",
                    retryable = true,
                )
            } finally {
                removalJob = null
            }
        }
    }

    private fun computeStatus(): CreationRuntimeStatus = if (installedFilesAreValid()) {
        CreationRuntimeStatus.Ready(installedBytes())
    } else {
        CreationRuntimeStatus.Missing
    }

    private fun installBundle() {
        val spec = requireNotNull(delivery) { "Creation engine is not included in this build" }
        if (installedFilesAreValid()) return
        val partial = bundlePartial()
        partial.parentFile?.mkdirs()
        partial.delete()
        val request = Request.Builder().url(spec.downloadUrl).build()
        httpClient.newCall(request).execute().use { response ->
            check(response.isSuccessful) { "Creation runtime HTTP ${response.code}" }
            val declared = response.body.contentLength()
            check(declared < 0L || declared == spec.sizeBytes) {
                "Creation runtime response has an unexpected size"
            }
            var downloaded = 0L
            FileOutputStream(partial).use { output ->
                response.body.byteStream().use { input ->
                    val buffer = ByteArray(BUFFER_BYTES)
                    while (true) {
                        val read = input.read(buffer)
                        if (read < 0) break
                        downloaded += read
                        check(downloaded <= spec.sizeBytes) {
                            "Creation runtime download is oversized"
                        }
                        output.write(buffer, 0, read)
                        mutableStatus.value = CreationRuntimeStatus.Downloading(
                            downloaded.toFloat() / spec.sizeBytes,
                        )
                    }
                    output.fd.sync()
                }
            }
        }
        check(validFile(partial, spec.sizeBytes, spec.sha256)) {
            "Creation runtime bundle failed validation"
        }
        extractBundle(partial)
        partial.delete()
        check(installedFilesAreValid()) { "Creation runtime files failed validation" }
    }

    private fun extractBundle(bundle: File) {
        val spec = requireNotNull(delivery)
        val staging = File(runtimeRoot(), ".install-${UUID.randomUUID()}").apply {
            deleteRuntimeTree(runtimeRoot(), this)
            mkdirs()
        }
        val targets = spec.entries.associate { entry ->
            entry.archivePath to safeInstalledFile(staging, entry.installPath)
        }
        val installed = mutableSetOf<String>()
        var entryCount = 0
        var uncompressedBytes = 0L
        try {
            ZipInputStream(bundle.inputStream().buffered()).use { zip ->
                while (true) {
                    val archiveEntry = zip.nextEntry ?: break
                    entryCount += 1
                    check(entryCount <= MAXIMUM_ARCHIVE_ENTRIES) {
                        "Creation runtime bundle has too many entries"
                    }
                    val target = targets[archiveEntry.name]
                    if (target != null && !archiveEntry.isDirectory) {
                        check(installed.add(archiveEntry.name)) {
                            "Creation runtime bundle contains a duplicate entry"
                        }
                        target.parentFile?.mkdirs()
                        val contract = spec.entries.single { it.archivePath == archiveEntry.name }
                        check(
                            contract.sizeBytes <= MAXIMUM_UNCOMPRESSED_BYTES - uncompressedBytes,
                        ) {
                            "Creation runtime bundle expands beyond its limit"
                        }
                        uncompressedBytes += copyBounded(zip, target, contract.sizeBytes)
                        check(validFile(target, contract.sizeBytes, contract.sha256)) {
                            "Creation runtime entry failed validation"
                        }
                        check(target.setReadOnly()) { "Could not lock ${target.name}" }
                    } else if (!archiveEntry.isDirectory ||
                        archiveEntry.name !in allowedArchiveDirectories(targets.keys)
                    ) {
                        error("Creation runtime bundle contains an unknown entry")
                    }
                    zip.closeEntry()
                }
            }
            check(installed == targets.keys) { "Creation runtime bundle is incomplete" }
            val destination = versionDirectory()
            deleteRuntimeTree(runtimeRoot(), destination)
            check(staging.renameTo(destination)) { "Could not commit creation runtime" }
            creationChildDirectoriesNoFollow(runtimeRoot())
                .filter { it != destination }
                .forEach { deleteRuntimeTree(runtimeRoot(), it) }
        } finally {
            deleteRuntimeTree(runtimeRoot(), staging)
        }
    }

    private fun loadFactory(): CreationRuntimeFactory? = runCatching {
        check(installedFilesAreValid()) { "Creation runtime is not installed" }
        val loader = DexClassLoader(
            runtimeDex().absolutePath,
            optimizedDirectory().apply { mkdirs() }.absolutePath,
            nativeLibrary().parentFile?.absolutePath,
            context.classLoader,
        )
        val type = Class.forName(requireNotNull(delivery).factoryClass, true, loader)
        type.getDeclaredConstructor().newInstance() as CreationRuntimeFactory
    }.getOrNull()

    private fun installedFilesAreValid(): Boolean {
        val spec = delivery ?: return false
        return spec.entries.all { entry ->
            validFile(
                safeInstalledFile(versionDirectory(), entry.installPath),
                entry.sizeBytes,
                entry.sha256,
            )
        }
    }

    private fun validFile(file: File, bytes: Long, sha256: String): Boolean {
        if (!file.isFile || file.length() != bytes) return false
        val digest = MessageDigest.getInstance("SHA-256")
        file.inputStream().use { input ->
            val buffer = ByteArray(BUFFER_BYTES)
            while (true) {
                val read = input.read(buffer)
                if (read < 0) break
                digest.update(buffer, 0, read)
            }
        }
        return digest.digest().joinToString("") { "%02x".format(it) } == sha256
    }

    private fun installedBytes(): Long =
        delivery?.entries?.sumOf { safeInstalledFile(versionDirectory(), it.installPath).length() }
            ?: 0L
    private fun runtimeRoot() = File(context.filesDir, "creation/runtime")
    private fun versionDirectory() = File(runtimeRoot(), requireNotNull(delivery).version)
    private fun runtimeDex() = installedEntry(ROLE_FACTORY_DEX)
    private fun nativeLibrary() = installedEntry(ROLE_NATIVE_LIBRARY)
    private fun installedEntry(role: String): File {
        val entry = requireNotNull(delivery).entry(role)
        return safeInstalledFile(versionDirectory(), entry.installPath)
    }
    private fun optimizedRoot() = File(context.codeCacheDir, "creation-runtime")
    private fun optimizedDirectory() =
        File(optimizedRoot(), requireNotNull(delivery).version)
    private fun bundlePartial() = File(
        context.cacheDir,
        "${delivery?.asset ?: "creation-runtime"}.part",
    )

    private companion object {
        const val BUFFER_BYTES = 128 * 1024
        const val MAXIMUM_ARCHIVE_ENTRIES = 64
        const val MAXIMUM_UNCOMPRESSED_BYTES = 1024L * 1024 * 1024
    }
}

private fun safeInstalledFile(root: File, relative: String): File {
    val rootPath = root.canonicalFile
    val target = File(rootPath, relative).canonicalFile
    require(target.path.startsWith(rootPath.path + File.separator)) {
        "Creation runtime entry escapes its install root"
    }
    return target
}

private fun copyBounded(input: ZipInputStream, target: File, expectedBytes: Long): Long {
    var written = 0L
    FileOutputStream(target).use { output ->
        val buffer = ByteArray(128 * 1024)
        while (true) {
            val read = input.read(buffer)
            if (read < 0) break
            written += read
            check(written <= expectedBytes) { "Creation runtime entry is oversized" }
            output.write(buffer, 0, read)
        }
        output.fd.sync()
        check(written == expectedBytes) { "Creation runtime entry is incomplete" }
    }
    return written
}

private fun deleteRuntimeTree(root: File, target: File) {
    check(!target.exists() || deleteCreationTreeNoFollow(root, target)) {
        "Creation runtime directory could not be removed safely"
    }
}

private fun allowedArchiveDirectories(paths: Set<String>): Set<String> = buildSet {
    paths.forEach { path ->
        var parent = path.substringBeforeLast('/', "")
        while (parent.isNotEmpty()) {
            add("$parent/")
            parent = parent.substringBeforeLast('/', "")
        }
    }
}
