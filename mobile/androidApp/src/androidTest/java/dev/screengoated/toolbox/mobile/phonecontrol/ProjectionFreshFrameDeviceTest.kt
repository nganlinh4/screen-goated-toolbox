package dev.screengoated.toolbox.mobile.phonecontrol

import android.content.Intent
import android.graphics.Bitmap
import android.graphics.Color
import android.os.SystemClock
import android.os.Build
import androidx.test.uiautomator.UiDevice
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import dev.screengoated.toolbox.mobile.phonecontrol.projection.PhoneControlProjectionFrameResult
import dev.screengoated.toolbox.mobile.phonecontrol.projection.PhoneControlProjectionProvider
import java.io.File
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class ProjectionFreshFrameDeviceTest {
    @Test fun smallChangeReplacesTheRetainedStaticFrame() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val context = instrumentation.targetContext
        context.startActivity(Intent(context, ProjectionCaptureFixtureActivity::class.java)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        try {
            val deadline = SystemClock.elapsedRealtime() + 120_000
            while (!PhoneControlProjectionProvider.isReady && SystemClock.elapsedRealtime() < deadline) {
                SystemClock.sleep(100)
            }
            assertTrue("Approve the Android screen-sharing consent dialog", PhoneControlProjectionProvider.isReady)
            SystemClock.sleep(500)
            val point = IntArray(2)
            instrumentation.runOnMainSync {
                val patch = requireNotNull(ProjectionCaptureFixtureActivity.instance).patch
                patch.getLocationOnScreen(point)
                point[0] += patch.width / 2
                point[1] += patch.height / 2
            }
            val first = capture()
            val dimensions = first.width to first.height
            assertEquals(Color.BLUE, first.getPixel(point[0], point[1]))
            save(first, "projection-blue.png", point)
            first.recycle()
            instrumentation.runOnMainSync {
                requireNotNull(ProjectionCaptureFixtureActivity.instance).patch.setBackgroundColor(Color.RED)
            }
            SystemClock.sleep(500)
            for (name in listOf("projection-red.png", "projection-static.png")) {
                val next = capture()
                assertEquals(Color.RED, next.getPixel(point[0], point[1]))
                save(next, name, point)
                next.recycle()
                SystemClock.sleep(1000)
            }
            if (Build.VERSION.SDK_INT < 34) {
                val device = UiDevice.getInstance(instrumentation)
                try {
                    device.setOrientationLeft()
                    SystemClock.sleep(2000)
                    instrumentation.runOnMainSync {
                        val patch = requireNotNull(ProjectionCaptureFixtureActivity.instance).patch
                        patch.getLocationOnScreen(point)
                        point[0] += patch.width / 2
                        point[1] += patch.height / 2
                    }
                    val rotated = capture()
                    assertEquals(dimensions.second, rotated.width)
                    assertEquals(dimensions.first, rotated.height)
                    assertEquals(Color.RED, rotated.getPixel(point[0], point[1]))
                    save(rotated, "projection-rotated.png", point)
                    File(context.cacheDir, "projection-rotation.txt").writeText(
                        "sdk=${Build.VERSION.SDK_INT} before=$dimensions after=${rotated.width}x${rotated.height} point=${point.toList()}",
                    )
                    rotated.recycle()
                } finally {
                    device.setOrientationNatural()
                }
            }
        } finally {
            instrumentation.runOnMainSync {
                PhoneControlProjectionProvider.stop()
                context.stopService(Intent(context, ProjectionCaptureFixtureService::class.java))
                ProjectionCaptureFixtureActivity.instance?.finish()
            }
        }
    }

    private fun capture(): Bitmap = runBlocking {
        val result = PhoneControlProjectionProvider.capture()
        assertTrue(result.toString(), result is PhoneControlProjectionFrameResult.Success)
        (result as PhoneControlProjectionFrameResult.Success).bitmap
    }

    private fun save(bitmap: Bitmap, name: String, point: IntArray) {
        val crop = Bitmap.createBitmap(bitmap, point[0] - 24, point[1] - 24, 48, 48)
        File(InstrumentationRegistry.getInstrumentation().targetContext.cacheDir, name)
            .outputStream().use { crop.compress(Bitmap.CompressFormat.PNG, 100, it) }
        crop.recycle()
    }
}
