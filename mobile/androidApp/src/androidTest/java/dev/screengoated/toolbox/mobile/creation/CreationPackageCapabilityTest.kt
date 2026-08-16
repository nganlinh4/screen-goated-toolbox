package dev.screengoated.toolbox.mobile.creation

import android.content.ComponentName
import android.content.pm.PackageManager
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import dev.screengoated.toolbox.mobile.creation.worker.ImageCreatorWorker0Service
import dev.screengoated.toolbox.mobile.creation.worker.ImageCreatorWorker1Service
import dev.screengoated.toolbox.mobile.creation.worker.ImageTo3dWorker0Service
import dev.screengoated.toolbox.mobile.creation.worker.ImageTo3dWorker1Service
import dev.screengoated.toolbox.mobile.creation.worker.ImageToSvgWorker0Service
import dev.screengoated.toolbox.mobile.creation.worker.ImageToSvgWorker1Service
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class CreationPackageCapabilityTest {
    @Test
    fun mergedPackageKeepsCreationActivityPrivateAndWorkersProcessIsolated() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val packageManager = context.packageManager
        val flags = PackageManager.ComponentInfoFlags.of(0)
        val activity = packageManager.getActivityInfo(
            ComponentName(context, CreationMiniAppActivity::class.java),
            flags,
        )
        val workers = listOf(
            ImageTo3dWorker0Service::class.java to ":sgt_creation_3d_0",
            ImageTo3dWorker1Service::class.java to ":sgt_creation_3d_1",
            ImageToSvgWorker0Service::class.java to ":sgt_creation_svg_0",
            ImageToSvgWorker1Service::class.java to ":sgt_creation_svg_1",
            ImageCreatorWorker0Service::class.java to ":sgt_creation_image_0",
            ImageCreatorWorker1Service::class.java to ":sgt_creation_image_1",
        ).map { (serviceClass, processSuffix) ->
            packageManager.getServiceInfo(ComponentName(context, serviceClass), flags).also {
                assertFalse(it.exported)
                assertTrue(it.enabled)
                assertEquals("${context.packageName}$processSuffix", it.processName)
            }
        }

        assertFalse(activity.exported)
        assertEquals(6, workers.map { it.processName }.distinct().size)
    }
}
