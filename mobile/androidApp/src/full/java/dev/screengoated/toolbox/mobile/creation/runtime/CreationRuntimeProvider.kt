package dev.screengoated.toolbox.mobile.creation.runtime

import android.content.Context
import dalvik.system.DexClassLoader
import java.io.File
import java.io.FileOutputStream
import java.security.MessageDigest
import java.util.zip.ZipInputStream
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import okhttp3.OkHttpClient
import okhttp3.Request

internal class CreationRuntimeProvider(private val context: Context) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val httpClient = OkHttpClient()
    private val mutableStatus = MutableStateFlow(computeStatus())
    private var installJob: Job? = null
    @Volatile private var loadedFactory: CreationRuntimeFactory? = null

    val status: StateFlow<CreationRuntimeStatus> = mutableStatus.asStateFlow()

    fun startInstall() {
        if (factory() != null || installJob?.isActive == true) return
        installJob = scope.launch {
            mutableStatus.value = CreationRuntimeStatus.Downloading(0f)
            try {
                installBundle()
                val factory = loadFactory() ?: error("Creation runtime could not be loaded")
                loadedFactory = factory
                mutableStatus.value = CreationRuntimeStatus.Ready(installedBytes())
            } catch (cancelled: CancellationException) {
                throw cancelled
            } catch (error: Throwable) {
                mutableStatus.value = CreationRuntimeStatus.Failed(
                    error.message ?: "Creation runtime installation failed",
                )
            } finally {
                installJob = null
            }
        }
    }

    fun factory(): CreationRuntimeFactory? {
        loadedFactory?.let { return it }
        if (!installedFilesAreValid()) return null
        return loadFactory()?.also {
            loadedFactory = it
            mutableStatus.value = CreationRuntimeStatus.Ready(installedBytes())
        }
    }

    fun delete() {
        installJob?.cancel()
        installJob = null
        loadedFactory = null
        runtimeDex().delete()
        nativeLibrary().delete()
        bundlePartial().delete()
        mutableStatus.value = CreationRuntimeStatus.Missing
    }

    private fun computeStatus(): CreationRuntimeStatus = if (installedFilesAreValid()) {
        CreationRuntimeStatus.Ready(installedBytes())
    } else {
        CreationRuntimeStatus.Missing
    }

    private fun installBundle() {
        if (installedFilesAreValid()) return
        val partial = bundlePartial()
        partial.parentFile?.mkdirs()
        partial.delete()
        val request = Request.Builder().url(RUNTIME_URL).build()
        httpClient.newCall(request).execute().use { response ->
            check(response.isSuccessful) { "Creation runtime HTTP ${response.code}" }
            val declared = response.body.contentLength()
            check(declared < 0L || declared == BUNDLE_BYTES) {
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
                        check(downloaded <= BUNDLE_BYTES) { "Creation runtime download is oversized" }
                        output.write(buffer, 0, read)
                        mutableStatus.value = CreationRuntimeStatus.Downloading(
                            downloaded.toFloat() / BUNDLE_BYTES,
                        )
                    }
                    output.fd.sync()
                }
            }
        }
        check(validFile(partial, BUNDLE_BYTES, BUNDLE_SHA256)) {
            "Creation runtime bundle failed validation"
        }
        extractBundle(partial)
        partial.delete()
        check(installedFilesAreValid()) { "Creation runtime files failed validation" }
    }

    private fun extractBundle(bundle: File) {
        val targets = mapOf(
            DEX_ENTRY to runtimeDex(),
            NATIVE_ENTRY to nativeLibrary(),
        )
        val installed = mutableSetOf<String>()
        ZipInputStream(bundle.inputStream().buffered()).use { zip ->
            while (true) {
                val entry = zip.nextEntry ?: break
                val target = targets[entry.name]
                if (target != null && !entry.isDirectory) {
                    target.parentFile?.mkdirs()
                    FileOutputStream(target).use { output -> zip.copyTo(output, BUFFER_BYTES) }
                    check(target.setReadOnly()) { "Could not lock ${target.name}" }
                    installed += entry.name
                }
                zip.closeEntry()
            }
        }
        check(installed == targets.keys) { "Creation runtime bundle is incomplete" }
    }

    private fun loadFactory(): CreationRuntimeFactory? = runCatching {
        check(installedFilesAreValid()) { "Creation runtime is not installed" }
        System.load(nativeLibrary().absolutePath)
        val loader = DexClassLoader(
            runtimeDex().absolutePath,
            optimizedDirectory().apply { mkdirs() }.absolutePath,
            nativeLibrary().parentFile?.absolutePath,
            context.classLoader,
        )
        val type = Class.forName(FACTORY_CLASS, true, loader)
        type.getDeclaredConstructor().newInstance() as CreationRuntimeFactory
    }.getOrNull()

    private fun installedFilesAreValid(): Boolean =
        validFile(runtimeDex(), DEX_BYTES, DEX_SHA256) &&
            validFile(nativeLibrary(), NATIVE_BYTES, NATIVE_SHA256)

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

    private fun installedBytes(): Long = runtimeDex().length() + nativeLibrary().length()
    private fun runtimeDirectory() = File(context.filesDir, "creation/runtime")
    private fun runtimeDex() = File(runtimeDirectory(), "sgt-creation-runtime.dex.jar")
    private fun nativeLibrary() = File(runtimeDirectory(), "lib/arm64-v8a/libsgt_creation_glb.so")
    private fun optimizedDirectory() = File(context.codeCacheDir, "creation-runtime")
    private fun bundlePartial() = File(context.cacheDir, "sgt-creation-runtime-android-arm64.zip.part")

    private companion object {
        const val FACTORY_CLASS =
            "dev.screengoated.toolbox.creation.runtime.AndroidCreationRuntimeFactory"
        const val RUNTIME_URL =
            "https://github.com/nganlinh4/screen-goated-toolbox/releases/download/sgt-runtime-bundles/sgt-creation-runtime-android-arm64.zip"
        const val BUNDLE_BYTES = 227_685L
        const val BUNDLE_SHA256 = "b30721f650b35480fc826f3a0619c6bd8571f24150b879b9fb073f8b959d5f77"
        const val DEX_ENTRY = "runtime/sgt-creation-runtime.dex.jar"
        const val DEX_BYTES = 110_640L
        const val DEX_SHA256 = "5f2111e848c3e70dcaf99c8e0632145b3c60e57f80bd788eac34487a72b2f0db"
        const val NATIVE_ENTRY = "lib/arm64-v8a/libsgt_creation_glb.so"
        const val NATIVE_BYTES = 342_304L
        const val NATIVE_SHA256 = "6520b51e703b953ffed3509310f693c68ceb107b37908dfac4c44eb9f42c55cc"
        const val BUFFER_BYTES = 128 * 1024
    }
}
