package dev.screengoated.toolbox.mobile.phonecontrol.provider.detector

import android.content.Context
import com.google.android.play.core.splitcompat.SplitCompat
import dev.screengoated.toolbox.mobile.service.nativelibs.NativeLibManager
import java.io.File
import java.io.FileOutputStream
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.withContext

/** Copies the detector supplied by the installed Play feature into its stable model path. */
internal object UiDetectorBundledModelSource {
    suspend fun copyTo(
        context: Context,
        partial: File,
    ): UiDetectorBundledModelResult = withContext(Dispatchers.IO) {
        val engine = NativeLibManager.Engine.ORT
        val manager = NativeLibManager(context)
        if (!manager.isInstalled(engine)) return@withContext UiDetectorBundledModelResult.Pending
        if (!SplitCompat.install(context)) {
            return@withContext UiDetectorBundledModelResult.Failed(
                "The installed detector feature could not be activated.",
            )
        }
        val assetContext = runCatching {
            context.createContextForSplit(engine.moduleName)
        }.getOrElse { context }
        partial.parentFile?.mkdirs()
        partial.delete()
        try {
            assetContext.assets.open(ASSET_PATH).use { input ->
                FileOutputStream(partial).use { output ->
                    val buffer = ByteArray(COPY_BUFFER_BYTES)
                    var copied = 0L
                    while (true) {
                        currentCoroutineContext().ensureActive()
                        val read = input.read(buffer)
                        if (read < 0) break
                        copied += read
                        check(copied <= UiDetectorContract.MODEL_BYTES) {
                            "Bundled UI detector exceeds its size contract"
                        }
                        output.write(buffer, 0, read)
                    }
                    output.fd.sync()
                }
            }
            UiDetectorBundledModelResult.Copied
        } catch (cancelled: CancellationException) {
            partial.delete()
            throw cancelled
        } catch (error: Throwable) {
            partial.delete()
            UiDetectorBundledModelResult.Failed(
                error.message ?: "The bundled UI detector could not be copied.",
            )
        }
    }
}

private const val ASSET_PATH = "ui_detector/ui-detr-1.onnx"
private const val COPY_BUFFER_BYTES = 1024 * 1_024
