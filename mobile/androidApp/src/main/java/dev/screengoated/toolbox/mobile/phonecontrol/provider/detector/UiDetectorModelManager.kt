package dev.screengoated.toolbox.mobile.phonecontrol.provider.detector

import android.content.Context
import dev.screengoated.toolbox.mobile.SgtMobileApplication
import dev.screengoated.toolbox.mobile.service.nativelibs.NativeLibManager
import java.io.File
import java.io.FileOutputStream
import java.io.IOException
import java.nio.file.Files
import java.nio.file.StandardCopyOption
import java.security.MessageDigest
import kotlin.coroutines.resume
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.suspendCancellableCoroutine
import okhttp3.Call
import okhttp3.Callback
import okhttp3.Request
import okhttp3.Response

internal sealed interface UiDetectorPreparation {
    data class Ready(val model: File) : UiDetectorPreparation

    data class Pending(
        val requiredUserStep: String,
        val message: String,
    ) : UiDetectorPreparation

    data class Failed(
        val code: String,
        val message: String,
        val retryable: Boolean,
    ) : UiDetectorPreparation
}

internal sealed interface UiDetectorDownloadState {
    data object Missing : UiDetectorDownloadState
    data class Downloading(val progress: Float) : UiDetectorDownloadState
    data object Ready : UiDetectorDownloadState
    data class Failed(val message: String) : UiDetectorDownloadState
}

internal sealed interface UiDetectorReadiness {
    data object Ready : UiDetectorReadiness
    data class Missing(val message: String) : UiDetectorReadiness
    data class Downloading(val message: String) : UiDetectorReadiness
    data class Failed(val message: String) : UiDetectorReadiness
}

/** One model contract; only NativeLibManager's delivery mechanism differs by flavor. */
internal class UiDetectorModelManager private constructor(
    private val context: Context,
) {
    private val httpClient =
        (context.applicationContext as SgtMobileApplication).appContainer.httpClient
    private val nativeLibManager = NativeLibManager(context)
    private val prepareMutex = Mutex()

    @Volatile
    var downloadState: UiDetectorDownloadState = UiDetectorDownloadState.Missing
        private set

    private var verifiedMetadata: Pair<Long, Long>? = null

    suspend fun prepare(): UiDetectorPreparation = prepareMutex.withLock {
        probeOrStartRuntime()
        val model = modelFile()
        val modelResult = when {
            validateModel(model) -> UiDetectorPreparation.Ready(model)
            else -> prepareMissingModel(model)
        }
        if (modelResult !is UiDetectorPreparation.Ready) return@withLock modelResult
        return@withLock when (val runtimeAfter = probeOrStartRuntime()) {
            RuntimeState.Ready -> modelResult
            is RuntimeState.Pending -> UiDetectorPreparation.Pending(
                requiredUserStep = "download_onnx_runtime",
                message = runtimeAfter.message,
            )
            is RuntimeState.Failed -> UiDetectorPreparation.Failed(
                code = "detector_runtime_unavailable",
                message = runtimeAfter.message,
                retryable = true,
            )
        }
    }

    private suspend fun prepareMissingModel(target: File): UiDetectorPreparation {
        verifiedMetadata = null
        target.parentFile?.mkdirs()
        val partial = File(target.parentFile, "${target.name}.part")
        partial.delete()
        return when (val bundled = UiDetectorBundledModelSource.copyTo(context, partial)) {
            UiDetectorBundledModelResult.Unavailable -> downloadModel(target)
            UiDetectorBundledModelResult.Pending -> {
                downloadState = UiDetectorDownloadState.Missing
                UiDetectorPreparation.Pending(
                    requiredUserStep = "download_onnx_runtime",
                    message = "The Play-delivered UI detector is not installed yet.",
                )
            }
            UiDetectorBundledModelResult.Copied -> finalizeModel(
                partial = partial,
                target = target,
                failureCode = "detector_model_invalid",
                failureMessage = "The bundled UI detector failed validation.",
            )
            is UiDetectorBundledModelResult.Failed -> {
                partial.delete()
                downloadState = UiDetectorDownloadState.Failed(bundled.message)
                UiDetectorPreparation.Failed(
                    code = "detector_delivery_failed",
                    message = bundled.message,
                    retryable = true,
                )
            }
        }
    }

    fun readiness(): UiDetectorReadiness {
        val modelPresent = modelFile().let { it.isFile && it.length() == UiDetectorContract.MODEL_BYTES }
        val runtimeStatus = nativeLibManager.status(NativeLibManager.Engine.ORT).value
        val detectorState = downloadState
        val runtimeLoadable = runtimeStatus is NativeLibManager.Status.Installed &&
            runCatching { NativeLibManager.ensureOrtLoaded(context) }.getOrDefault(false)
        return when {
            detectorState is UiDetectorDownloadState.Failed -> UiDetectorReadiness.Failed(
                detectorState.message,
            )
            runtimeStatus is NativeLibManager.Status.Error ->
                UiDetectorReadiness.Failed(runtimeStatus.message)
            detectorState is UiDetectorDownloadState.Downloading -> {
                val progress = detectorState.progress
                UiDetectorReadiness.Downloading("UI detector is downloading (${(progress * 100).toInt()}%).")
            }
            runtimeStatus is NativeLibManager.Status.Downloading -> UiDetectorReadiness.Downloading(
                "ONNX Runtime is downloading (${(runtimeStatus.progress * 100).toInt()}%).",
            )
            modelPresent && runtimeLoadable -> UiDetectorReadiness.Ready
            modelPresent && runtimeStatus is NativeLibManager.Status.Installed ->
                UiDetectorReadiness.Failed("ONNX Runtime could not be loaded on this device.")
            !modelPresent -> UiDetectorReadiness.Missing("The on-demand UI detector is not downloaded.")
            else -> UiDetectorReadiness.Missing("The on-demand ONNX Runtime is not installed.")
        }
    }

    private fun probeOrStartRuntime(): RuntimeState {
        val loaded = runCatching { NativeLibManager.ensureOrtLoaded(context) }.getOrDefault(false)
        if (loaded) return RuntimeState.Ready
        nativeLibManager.startDownload(NativeLibManager.Engine.ORT)
        return when (val status = nativeLibManager.status(NativeLibManager.Engine.ORT).value) {
            is NativeLibManager.Status.Error -> RuntimeState.Failed(status.message)
            is NativeLibManager.Status.Downloading -> RuntimeState.Pending(
                "ONNX Runtime is downloading (${(status.progress * 100).toInt()}%).",
            )
            is NativeLibManager.Status.Installed -> RuntimeState.Pending(
                "ONNX Runtime is installed but could not be loaded on this device.",
            )
            NativeLibManager.Status.Missing -> RuntimeState.Pending("ONNX Runtime download started.")
        }
    }

    private suspend fun downloadModel(target: File): UiDetectorPreparation {
        verifiedMetadata = null
        target.parentFile?.mkdirs()
        val partial = File(target.parentFile, "${target.name}.part")
        partial.delete()
        downloadState = UiDetectorDownloadState.Downloading(0f)
        val result = downloadValidated(partial)
        if (result.isFailure) {
            partial.delete()
            val message = result.exceptionOrNull()?.message ?: "UI detector download failed."
            downloadState = UiDetectorDownloadState.Failed(message)
            return UiDetectorPreparation.Failed(
                code = "detector_download_failed",
                message = message,
                retryable = true,
            )
        }
        return finalizeModel(
            partial = partial,
            target = target,
            failureCode = "detector_model_invalid",
            failureMessage = "Downloaded UI detector failed final validation.",
        )
    }

    private fun finalizeModel(
        partial: File,
        target: File,
        failureCode: String,
        failureMessage: String,
    ): UiDetectorPreparation = try {
        moveAtomically(partial, target)
        check(validateModel(target)) { failureMessage }
        downloadState = UiDetectorDownloadState.Ready
        UiDetectorPreparation.Ready(target)
    } catch (error: Throwable) {
        partial.delete()
        target.delete()
        val message = error.message ?: failureMessage
        downloadState = UiDetectorDownloadState.Failed(message)
        UiDetectorPreparation.Failed(failureCode, message, retryable = true)
    }

    private suspend fun downloadValidated(target: File): Result<Unit> =
        suspendCancellableCoroutine { continuation ->
            val request = Request.Builder().url(UiDetectorContract.MODEL_URL).build()
            val call = httpClient.newCall(request)
            continuation.invokeOnCancellation {
                call.cancel()
                target.delete()
            }
            call.enqueue(
                object : Callback {
                    override fun onFailure(call: Call, e: IOException) {
                        target.delete()
                        if (continuation.isActive) continuation.resume(Result.failure(e))
                    }

                    override fun onResponse(call: Call, response: Response) {
                        val result = runCatching {
                            response.use { checked ->
                                if (!checked.isSuccessful) error("UI detector HTTP ${checked.code}")
                                val declared = checked.body.contentLength()
                                if (declared > UiDetectorContract.MODEL_BYTES) {
                                    error("UI detector response exceeds the expected size")
                                }
                                val digest = MessageDigest.getInstance("SHA-256")
                                var downloaded = 0L
                                checked.body.byteStream().use { input ->
                                    FileOutputStream(target).use { output ->
                                        val buffer = ByteArray(DOWNLOAD_BUFFER_BYTES)
                                        while (true) {
                                            val read = input.read(buffer)
                                            if (read < 0) break
                                            if (!continuation.isActive) throw IOException("download cancelled")
                                            downloaded += read
                                            if (downloaded > UiDetectorContract.MODEL_BYTES) {
                                                error("UI detector download exceeded expected size")
                                            }
                                            output.write(buffer, 0, read)
                                            digest.update(buffer, 0, read)
                                            downloadState = UiDetectorDownloadState.Downloading(
                                                downloaded.toFloat() / UiDetectorContract.MODEL_BYTES,
                                            )
                                        }
                                        output.fd.sync()
                                    }
                                }
                                check(downloaded == UiDetectorContract.MODEL_BYTES) {
                                    "UI detector size $downloaded does not match ${UiDetectorContract.MODEL_BYTES}"
                                }
                                check(digest.digest().toHex() == UiDetectorContract.MODEL_SHA256) {
                                    "UI detector checksum mismatch"
                                }
                            }
                        }
                        if (result.isFailure) target.delete()
                        if (continuation.isActive) continuation.resume(result)
                    }
                },
            )
        }

    @Synchronized
    private fun validateModel(file: File): Boolean {
        if (!file.isFile || file.length() != UiDetectorContract.MODEL_BYTES) return false
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
        val valid = digest.digest().toHex() == UiDetectorContract.MODEL_SHA256
        verifiedMetadata = metadata.takeIf { valid }
        downloadState = if (valid) UiDetectorDownloadState.Ready else UiDetectorDownloadState.Missing
        return valid
    }

    private fun modelFile(): File =
        File(context.filesDir, "models/ui-detector/ui-detr-1.onnx")

    private sealed interface RuntimeState {
        data object Ready : RuntimeState
        data class Pending(val message: String) : RuntimeState
        data class Failed(val message: String) : RuntimeState
    }

    companion object {
        @Volatile private var instance: UiDetectorModelManager? = null

        fun get(context: Context): UiDetectorModelManager = instance ?: synchronized(this) {
            instance ?: UiDetectorModelManager(context.applicationContext).also { instance = it }
        }
    }
}

private fun moveAtomically(source: File, target: File) {
    runCatching {
        Files.move(
            source.toPath(),
            target.toPath(),
            StandardCopyOption.ATOMIC_MOVE,
            StandardCopyOption.REPLACE_EXISTING,
        )
    }.getOrElse {
        Files.move(source.toPath(), target.toPath(), StandardCopyOption.REPLACE_EXISTING)
    }
}

private fun ByteArray.toHex(): String = joinToString("") { byte -> "%02x".format(byte) }

private const val DOWNLOAD_BUFFER_BYTES = 128 * 1_024
private const val VALIDATION_BUFFER_BYTES = 1024 * 1_024
