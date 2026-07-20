package dev.screengoated.toolbox.mobile.phonecontrol

import android.Manifest
import android.accessibilityservice.AccessibilityServiceInfo
import android.content.ComponentName
import android.content.Context
import android.content.pm.PackageManager
import android.content.pm.ServiceInfo
import android.os.Build
import android.view.accessibility.AccessibilityManager
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import dev.screengoated.toolbox.mobile.phonecontrol.ui.PhoneControlActivity
import dev.screengoated.toolbox.mobile.service.SgtAccessibilityService
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class PhoneControlPackageCapabilityTest {
    @Test
    fun mergedPackageDeclaresPrivatePhoneControlComponentsAndPermissions() {
        assumeTrue(Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE)
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val packageManager = context.packageManager
        val activity = packageManager.getActivityInfo(
            ComponentName(context, PhoneControlActivity::class.java),
            PackageManager.ComponentInfoFlags.of(0),
        )
        val service = packageManager.getServiceInfo(
            ComponentName(context, PhoneControlService::class.java),
            PackageManager.ComponentInfoFlags.of(0),
        )
        val debugProbe = packageManager.getReceiverInfo(
            ComponentName(context, PhoneControlDebugProbeReceiver::class.java),
            PackageManager.ComponentInfoFlags.of(0),
        )
        val receiptCleanup = packageManager.getReceiverInfo(
            ComponentName(context, PhoneControlDebugProbeCleanupReceiver::class.java),
            PackageManager.ComponentInfoFlags.of(0),
        )
        val packageInfo = packageManager.getPackageInfo(
            context.packageName,
            PackageManager.PackageInfoFlags.of(PackageManager.GET_PERMISSIONS.toLong()),
        )

        assertFalse(activity.exported)
        assertFalse(service.exported)
        assertTrue(debugProbe.exported)
        assertEquals(Manifest.permission.DUMP, debugProbe.permission)
        assertFalse(receiptCleanup.exported)
        assertTrue(service.enabled)
        val expectedTypes = ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE or
            ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE or
            ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK or
            ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PROJECTION
        assertEquals(expectedTypes, service.foregroundServiceType)
        assertTrue(
            packageInfo.requestedPermissions.orEmpty().contains(Manifest.permission.SYSTEM_ALERT_WINDOW),
        )
    }

    @Test
    fun installedAccessibilityDeclarationProvidesPhoneControlCapabilities() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val manager = context.getSystemService(Context.ACCESSIBILITY_SERVICE) as AccessibilityManager
        val declaration = manager.installedAccessibilityServiceList.single { info ->
            val service = info.resolveInfo.serviceInfo
            service.packageName == context.packageName &&
                service.name == SgtAccessibilityService::class.java.name
        }
        val expected = AccessibilityServiceInfo.CAPABILITY_CAN_RETRIEVE_WINDOW_CONTENT or
            AccessibilityServiceInfo.CAPABILITY_CAN_PERFORM_GESTURES or
            AccessibilityServiceInfo.CAPABILITY_CAN_TAKE_SCREENSHOT

        assertEquals(expected, declaration.capabilities and expected)
    }
}
