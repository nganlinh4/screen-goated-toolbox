package dev.screengoated.toolbox.mobile.phonecontrol

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import dev.screengoated.toolbox.mobile.phonecontrol.provider.detector.UiDetectorContract
import dev.screengoated.toolbox.mobile.phonecontrol.provider.detector.UiDetectorModelManager
import dev.screengoated.toolbox.mobile.phonecontrol.provider.detector.UiDetectorOnnxEngine
import dev.screengoated.toolbox.mobile.phonecontrol.provider.detector.UiDetectorPreparation
import dev.screengoated.toolbox.mobile.service.nativelibs.NativeLibManager
import dev.screengoated.toolbox.mobile.service.nativelibs.NativeRuntimeContract
import dev.screengoated.toolbox.mobile.service.nativelibs.VerifiedNativeArchive
import java.io.File
import java.security.MessageDigest
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class FullPhoneControlDetectorDeliveryTest {
    @Test
    fun cleanInstallExtractsExactRuntimeAndRunsOnCurrentDeviceFrame() = runBlocking {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val context = instrumentation.targetContext
        val nativeLibs = NativeLibManager(context)
        val nativeDir = File(context.filesDir, "native-libs")
        val modelFile = File(context.filesDir, "models/ui-detector/ui-detr-1.onnx")
        assertFalse("ORT must not be extracted before the request", nativeLibs.isInstalled(NativeLibManager.Engine.ORT))
        assertFalse("UI detector must be absent on a clean install", modelFile.exists())

        val manifest = NativeRuntimeContract.load(context)
        val ort = manifest.archive("ort")
        assertEquals("bundled_asset", ort.fullDelivery)
        val assetDigest = MessageDigest.getInstance("SHA-256")
        var assetBytes = 0L
        context.assets.open(NativeRuntimeContract.FULL_ORT_ASSET_PATH).use { input ->
            val buffer = ByteArray(1024 * 1024)
            while (true) {
                val read = input.read(buffer)
                if (read < 0) break
                assetBytes += read
                assetDigest.update(buffer, 0, read)
            }
        }
        assertEquals(ort.byteCount, assetBytes)
        assertEquals(ort.sha256, assetDigest.digest().hex())

        nativeLibs.startDownload(NativeLibManager.Engine.ORT)
        val terminal = withTimeout(RUNTIME_TIMEOUT_MS) {
            nativeLibs.status(NativeLibManager.Engine.ORT).first { status ->
                status is NativeLibManager.Status.Installed || status is NativeLibManager.Status.Error
            }
        }
        if (terminal is NativeLibManager.Status.Error) {
            throw AssertionError("Full bundled ORT extraction failed: ${terminal.message}")
        }
        assertTrue(nativeLibs.isInstalled(NativeLibManager.Engine.ORT))
        assertTrue(VerifiedNativeArchive.isInstalled(nativeDir, ort))
        assertEquals(ort.entries.map { it.fileName }.toSet(), nativeDir.list()!!.toSet())
        assertTrue(NativeLibManager.ensureOrtLoaded(context))

        val prepared = withTimeout(MODEL_TIMEOUT_MS) { UiDetectorModelManager.get(context).prepare() }
        val model = when (prepared) {
            is UiDetectorPreparation.Ready -> prepared.model
            is UiDetectorPreparation.Pending -> throw AssertionError(
                "Detector remained pending: ${prepared.message}",
            )
            is UiDetectorPreparation.Failed -> throw AssertionError(
                "Detector preparation failed (${prepared.code}): ${prepared.message}",
            )
        }
        assertEquals(UiDetectorContract.MODEL_BYTES, model.length())
        assertEquals(UiDetectorContract.MODEL_SHA256, sha256(model))

        val frame = requireNotNull(instrumentation.uiAutomation.takeScreenshot()) {
            "Could not capture the current device frame"
        }
        try {
            val inference = withTimeout(INFERENCE_TIMEOUT_MS) {
                UiDetectorOnnxEngine.detect(frame, 0, 0, model)
            }
            assertEquals("cpu", inference.executionProvider)
            assertTrue(inference.durationMs >= 0L)
            assertTrue(inference.output.stats.thresholded >= 0)
        } finally {
            frame.recycle()
        }
    }
}

private fun sha256(file: File): String {
    val digest = MessageDigest.getInstance("SHA-256")
    file.inputStream().use { input ->
        val buffer = ByteArray(1024 * 1024)
        while (true) {
            val read = input.read(buffer)
            if (read < 0) break
            digest.update(buffer, 0, read)
        }
    }
    return digest.digest().hex()
}

private fun ByteArray.hex(): String = joinToString("") { byte -> "%02x".format(byte) }

private const val RUNTIME_TIMEOUT_MS = 30_000L
private const val MODEL_TIMEOUT_MS = 180_000L
private const val INFERENCE_TIMEOUT_MS = 180_000L
