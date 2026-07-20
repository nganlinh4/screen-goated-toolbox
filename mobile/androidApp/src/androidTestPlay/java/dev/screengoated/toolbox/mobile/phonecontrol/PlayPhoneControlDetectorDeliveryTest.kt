package dev.screengoated.toolbox.mobile.phonecontrol

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.google.android.play.core.splitinstall.SplitInstallManagerFactory
import dev.screengoated.toolbox.mobile.phonecontrol.provider.detector.UiDetectorContract
import dev.screengoated.toolbox.mobile.phonecontrol.provider.detector.UiDetectorModelManager
import dev.screengoated.toolbox.mobile.phonecontrol.provider.detector.UiDetectorOnnxEngine
import dev.screengoated.toolbox.mobile.phonecontrol.provider.detector.UiDetectorPreparation
import dev.screengoated.toolbox.mobile.service.nativelibs.NativeLibManager
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
class PlayPhoneControlDetectorDeliveryTest {
    @Test
    fun localTestingSplitDeliversRuntimeModelAndRunsOnCurrentDeviceFrame() = runBlocking {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val context = instrumentation.targetContext
        val nativeLibs = NativeLibManager(context)
        assertFalse(
            "ORT must be absent before the on-demand request",
            nativeLibs.isInstalled(NativeLibManager.Engine.ORT),
        )
        assertFalse(
            java.io.File(context.filesDir, "models/ui-detector/ui-detr-1.onnx").exists(),
        )
        val localTestingDirectory = File(
            requireNotNull(context.getExternalFilesDir(null)) {
                "External files directory is unavailable"
            },
            "local_testing",
        )
        val localTestingFiles = localTestingDirectory.list()?.sorted().orEmpty()
        assertTrue(
            "Local-testing splits are unavailable to the app " +
                "(path=${localTestingDirectory.absolutePath}, " +
                "directory=${localTestingDirectory.isDirectory}, " +
                "readable=${localTestingDirectory.canRead()}, files=$localTestingFiles)",
            localTestingDirectory.isDirectory &&
                localTestingDirectory.canRead() &&
                localTestingFiles.isNotEmpty(),
        )
        nativeLibs.startDownload(NativeLibManager.Engine.ORT)
        val terminal = withTimeout(SPLIT_TIMEOUT_MS) {
            nativeLibs.status(NativeLibManager.Engine.ORT).first { status ->
                status is NativeLibManager.Status.Installed || status is NativeLibManager.Status.Error
            }
        }
        if (terminal is NativeLibManager.Status.Error) {
            throw AssertionError("Play ORT split delivery failed: ${terminal.message}")
        }
        assertTrue(
            "ORT and shared C++ modules must both be installed",
            nativeLibs.isInstalled(NativeLibManager.Engine.ORT),
        )
        val installedModules = SplitInstallManagerFactory.create(context).installedModules
        assertTrue(
            "feature_asr_ort is absent from Play Core modules: $installedModules",
            installedModules.contains("feature_asr_ort"),
        )
        assertTrue(
            "feature_native_cpp is absent from Play Core modules: $installedModules",
            installedModules.contains("feature_native_cpp"),
        )
        assertTrue(NativeLibManager.ensureOrtLoaded(context))

        val prepared = withTimeout(MODEL_TIMEOUT_MS) {
            UiDetectorModelManager.get(context).prepare()
        }
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

private fun sha256(file: java.io.File): String {
    val digest = MessageDigest.getInstance("SHA-256")
    file.inputStream().use { input ->
        val buffer = ByteArray(1024 * 1_024)
        while (true) {
            val read = input.read(buffer)
            if (read < 0) break
            digest.update(buffer, 0, read)
        }
    }
    return digest.digest().joinToString("") { byte -> "%02x".format(byte) }
}

private const val SPLIT_TIMEOUT_MS = 120_000L
private const val MODEL_TIMEOUT_MS = 120_000L
private const val INFERENCE_TIMEOUT_MS = 180_000L
