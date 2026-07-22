package dev.screengoated.toolbox.mobile.creation

import android.content.Context
import dev.screengoated.toolbox.mobile.SgtMobileApplication
import dev.screengoated.toolbox.mobile.creation.runtime.CreationDepthRuntime
import dev.screengoated.toolbox.mobile.creation.runtime.CreationRuntimeManager
import dev.screengoated.toolbox.mobile.service.nativelibs.NativeLibManager
import java.io.File
import java.io.FileOutputStream
import java.security.MessageDigest
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withTimeoutOrNull
import okhttp3.Request

internal sealed interface DepthPreviewModelStatus {
    data object Missing : DepthPreviewModelStatus
    data class Downloading(val progress: Float) : DepthPreviewModelStatus
    data class Ready(val sizeBytes: Long) : DepthPreviewModelStatus
    data class Failed(val message: String) : DepthPreviewModelStatus
}

/** Shared, best-effort preview preparation for both creation mini apps. */
internal class DepthPreviewModelManager private constructor(
    private val context: Context,
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val httpClient =
        (context.applicationContext as SgtMobileApplication).appContainer.httpClient
    private val nativeLibManager = NativeLibManager(context)
    private val creationRuntimeManager = CreationRuntimeManager.get(context)
    private val modelMutex = Mutex()
    private val inferenceMutex = Mutex()
    private val mutableStatus = MutableStateFlow<DepthPreviewModelStatus>(DepthPreviewModelStatus.Missing)
    private var verifiedMetadata: Pair<Long, Long>? = null
    private var depthRuntime: CreationDepthRuntime? = null

    val status: StateFlow<DepthPreviewModelStatus> = mutableStatus.asStateFlow()

    init {
        scope.launch { probeExistingModel() }
    }

    fun startInstall() {
        creationRuntimeManager.startInstall()
        nativeLibManager.startDownload(NativeLibManager.Engine.ORT)
        scope.launch { runCatching { ensureModel() } }
    }

    fun delete() {
        scope.launch {
            inferenceMutex.withLock {
                modelMutex.withLock {
                    depthRuntime?.close()
                    depthRuntime = null
                    modelFile().delete()
                    previewDirectory().deleteRecursively()
                    verifiedMetadata = null
                    mutableStatus.value = DepthPreviewModelStatus.Missing
                }
            }
        }
    }

    suspend fun createPreview(sourcePath: String): String? = try {
        val modelTask = scope.async { ensureModel() }
        val runtimeTask = scope.async { creationRuntimeManager.awaitFactory() }
        nativeLibManager.startDownload(NativeLibManager.Engine.ORT)
        val model = modelTask.await()
        val factory = runtimeTask.await()
        if (factory == null || !awaitRuntime()) {
            null
        } else {
            inferenceMutex.withLock {
                val target = previewFile(sourcePath)
                if (target.isFile && target.length() > 0L) return@withLock target.absolutePath
                val runtime = depthRuntime ?: factory.createDepthRuntime().also { depthRuntime = it }
                check(runtime.createPreview(sourcePath, model.absolutePath, target.absolutePath)) {
                    "Depth preview was not created"
                }
                prunePreviews(target)
                target.absolutePath
            }
        }
    } catch (cancelled: CancellationException) {
        throw cancelled
    } catch (_: Throwable) {
        null
    }

    private suspend fun awaitRuntime(): Boolean {
        if (runCatching { NativeLibManager.ensureOrtLoaded(context) }.getOrDefault(false)) return true
        nativeLibManager.startDownload(NativeLibManager.Engine.ORT)
        val terminal = withTimeoutOrNull(RUNTIME_WAIT_MS) {
            nativeLibManager.status(NativeLibManager.Engine.ORT).first {
                it is NativeLibManager.Status.Installed || it is NativeLibManager.Status.Error
            }
        } ?: return false
        return terminal is NativeLibManager.Status.Installed &&
            runCatching { NativeLibManager.ensureOrtLoaded(context) }.getOrDefault(false)
    }

    private suspend fun probeExistingModel() = modelMutex.withLock {
        val model = modelFile()
        mutableStatus.value = if (validateModel(model)) {
            DepthPreviewModelStatus.Ready(model.length())
        } else {
            DepthPreviewModelStatus.Missing
        }
    }

    private suspend fun ensureModel(): File = modelMutex.withLock {
        val target = modelFile()
        if (validateModel(target)) {
            mutableStatus.value = DepthPreviewModelStatus.Ready(target.length())
            return@withLock target
        }
        verifiedMetadata = null
        target.parentFile?.mkdirs()
        val partial = File(target.parentFile, "${target.name}.part")
        partial.delete()
        mutableStatus.value = DepthPreviewModelStatus.Downloading(0f)
        try {
            downloadModel(partial)
            target.delete()
            check(partial.renameTo(target) || copyThenDelete(partial, target)) {
                "Could not install the depth preview model"
            }
            check(validateModel(target)) { "Depth preview model failed final validation" }
            mutableStatus.value = DepthPreviewModelStatus.Ready(target.length())
            target
        } catch (error: Throwable) {
            partial.delete()
            target.delete()
            val message = error.message ?: "Depth preview model download failed"
            mutableStatus.value = DepthPreviewModelStatus.Failed(message)
            throw error
        }
    }

    private fun downloadModel(target: File) {
        val request = Request.Builder().url(DepthPreviewContract.MODEL_URL).build()
        httpClient.newCall(request).execute().use { response ->
            check(response.isSuccessful) { "Depth preview model HTTP ${response.code}" }
            val declared = response.body.contentLength()
            check(declared < 0L || declared == DepthPreviewContract.MODEL_BYTES) {
                "Depth preview model response has an unexpected size"
            }
            val digest = MessageDigest.getInstance("SHA-256")
            var downloaded = 0L
            response.body.byteStream().use { input ->
                FileOutputStream(target).use { output ->
                    val buffer = ByteArray(DOWNLOAD_BUFFER_BYTES)
                    while (true) {
                        val read = input.read(buffer)
                        if (read < 0) break
                        downloaded += read
                        check(downloaded <= DepthPreviewContract.MODEL_BYTES) {
                            "Depth preview model exceeded its expected size"
                        }
                        output.write(buffer, 0, read)
                        digest.update(buffer, 0, read)
                        mutableStatus.value = DepthPreviewModelStatus.Downloading(
                            downloaded.toFloat() / DepthPreviewContract.MODEL_BYTES,
                        )
                    }
                    output.fd.sync()
                }
            }
            check(downloaded == DepthPreviewContract.MODEL_BYTES) {
                "Depth preview model download is incomplete"
            }
            check(digest.digest().toHex() == DepthPreviewContract.MODEL_SHA256) {
                "Depth preview model checksum mismatch"
            }
        }
    }

    @Synchronized
    private fun validateModel(file: File): Boolean {
        if (!file.isFile || file.length() != DepthPreviewContract.MODEL_BYTES) return false
        val metadata = file.length() to file.lastModified()
        if (verifiedMetadata == metadata) return true
        val digest = MessageDigest.getInstance("SHA-256")
        file.inputStream().use { input ->
            val buffer = ByteArray(VALIDATION_BUFFER_BYTES)
            while (true) {
                val read = input.read(buffer)
                if (read < 0) break
                digest.update(buffer, 0, read)
            }
        }
        return (digest.digest().toHex() == DepthPreviewContract.MODEL_SHA256).also { valid ->
            if (valid) verifiedMetadata = metadata
        }
    }

    private fun modelFile() = File(
        context.filesDir,
        "creation/depth-preview/models/${DepthPreviewContract.MODEL_NAME}",
    )

    private fun previewDirectory() = File(context.cacheDir, "creation/depth-previews")

    private fun previewFile(sourcePath: String): File {
        val source = File(sourcePath)
        val identity = "$sourcePath:${source.length()}:${source.lastModified()}"
        val key = MessageDigest.getInstance("SHA-256")
            .digest(identity.toByteArray())
            .toHex()
            .take(24)
        return File(previewDirectory().apply { mkdirs() }, "$key.png")
    }

    private fun prunePreviews(keep: File) {
        previewDirectory().listFiles()
            .orEmpty()
            .filter { it.isFile && it != keep }
            .sortedByDescending(File::lastModified)
            .drop(MAXIMUM_CACHED_PREVIEWS - 1)
            .forEach(File::delete)
    }

    private fun copyThenDelete(source: File, target: File): Boolean = runCatching {
        source.copyTo(target, overwrite = true)
        source.delete()
        true
    }.getOrDefault(false)

    companion object {
        private const val DOWNLOAD_BUFFER_BYTES = 1024 * 1024
        private const val VALIDATION_BUFFER_BYTES = 1024 * 1024
        private const val RUNTIME_WAIT_MS = 5 * 60 * 1000L
        private const val MAXIMUM_CACHED_PREVIEWS = 32

        @Volatile private var instance: DepthPreviewModelManager? = null

        fun get(context: Context): DepthPreviewModelManager = instance ?: synchronized(this) {
            instance ?: DepthPreviewModelManager(context.applicationContext).also { instance = it }
        }
    }
}

private fun ByteArray.toHex(): String = joinToString("") { "%02x".format(it) }
