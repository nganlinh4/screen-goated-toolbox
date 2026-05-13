package dev.screengoated.toolbox.mobile

import android.Manifest
import android.content.Intent
import android.graphics.Point
import android.os.Build
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.By
import androidx.test.uiautomator.UiDevice
import androidx.test.uiautomator.UiObject2
import androidx.test.uiautomator.Until
import org.junit.Assert.assertNotNull
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class SgtMobileE2eSmokeTest {
    private lateinit var device: UiDevice
    private val packageName = "dev.screengoated.toolbox.mobile.debug"

    @Before
    fun launchApp() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        grantRuntimePermission(Manifest.permission.RECORD_AUDIO)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            grantRuntimePermission(Manifest.permission.POST_NOTIFICATIONS)
        }

        device = UiDevice.getInstance(instrumentation)
        device.pressHome()

        val context = ApplicationProvider.getApplicationContext<android.content.Context>()
        val intent = context.packageManager.getLaunchIntentForPackage(packageName)
            ?: error("No launcher intent for $packageName")
        intent.addFlags(Intent.FLAG_ACTIVITY_CLEAR_TASK or Intent.FLAG_ACTIVITY_NEW_TASK)
        context.startActivity(intent)

        assertNotNull(device.wait(Until.hasObject(By.pkg(packageName).depth(0)), 10_000))
        waitForTag("sgt-app-root")
    }

    @Test
    fun shellTabsNavigateOnTabletEmulator() {
        listOf("apps", "tools", "settings", "history").forEach { section ->
            waitForTag("shell-tab-$section").click()
            waitForTag("shell-section-$section")
        }
    }

    @Test
    fun bundledMiniAppsOpenAndReturnToShell() {
        openMiniAppAndReturn(
            cardTag = "app-card-video-downloader",
            screenTag = "downloader-screen",
        )
        openMiniAppAndReturn(
            cardTag = "app-card-translation-gummy",
            screenTag = "translation-gummy-screen",
        )
        openMiniAppAndReturn(
            cardTag = "app-card-dj",
            screenTag = "dj-screen",
        )
    }

    @Test
    fun liveTranslateToggleStaysInShellOnTabletEmulator() {
        waitForTag("shell-tab-apps").click()
        waitForTag("shell-section-apps")
        waitForTag("live-translate-toggle").click()
        dismissRuntimePermissionDialogIfPresent()
        waitForTag("sgt-app-root")
        waitForTag("shell-section-apps")
    }

    @Test
    fun translationGummyVolumeControlsAreReachableOnTabletEmulator() {
        waitForTag("shell-tab-apps").click()
        waitForTag("shell-section-apps")
        waitForTag("app-card-translation-gummy").click()
        dismissTranslationGummyGuideIfPresent()
        waitForTag("translation-gummy-screen")
        waitForTag("translation-gummy-tts-settings").click()
        waitForTag("translation-gummy-volume-slider", timeoutMillis = 10_000).dragToCenter()
        waitForTag("translation-gummy-volume-mute").click()
        waitForTag("translation-gummy-volume-value")
        device.pressBack()
        waitForTag("translation-gummy-screen")
    }

    private fun openMiniAppAndReturn(
        cardTag: String,
        screenTag: String,
        backPresses: Int = 1,
    ) {
        waitForTag("shell-tab-apps").click()
        waitForTag("shell-section-apps")
        waitForTag(cardTag).click()
        dismissRuntimePermissionDialogIfPresent()
        dismissTranslationGummyGuideIfPresent()
        waitForTag(screenTag, timeoutMillis = 10_000)
        repeat(backPresses) {
            device.pressBack()
            device.waitForIdle(1_000)
        }
        waitForTag("sgt-app-root")
        waitForTagGone(screenTag)
    }

    private fun grantRuntimePermission(permission: String) {
        InstrumentationRegistry.getInstrumentation().uiAutomation
            .executeShellCommand("pm grant $packageName $permission")
            .use { it.close() }
    }

    private fun dismissRuntimePermissionDialogIfPresent() {
        val denyButton = device.wait(
            Until.findObject(By.res("com.android.permissioncontroller", "permission_deny_button")),
            1_000,
        ) ?: device.findObject(By.text("Don\u2019t allow"))
            ?: device.findObject(By.text("Don't allow"))
            ?: device.findObject(By.text("Deny"))

        denyButton?.click()
    }

    private fun dismissTranslationGummyGuideIfPresent() {
        val confirmButton = listOf("Got it!", "\u0110\u00e3 hi\u1ec3u!", "\uc54c\uaca0\uc2b5\ub2c8\ub2e4!")
            .firstNotNullOfOrNull { label -> device.findObject(By.text(label)) }

        confirmButton?.click()
        if (confirmButton != null) {
            device.waitForIdle(1_000)
        }
    }

    private fun waitForTag(
        tag: String,
        timeoutMillis: Long = 5_000,
    ): UiObject2 {
        return device.wait(Until.findObject(By.res(packageName, tag)), timeoutMillis)
            ?: device.wait(Until.findObject(By.res(tag)), timeoutMillis)
            ?: error("Timed out waiting for Compose test tag: $tag")
    }

    private fun waitForTagGone(
        tag: String,
        timeoutMillis: Long = 5_000,
    ) {
        if (!device.wait(Until.gone(By.res(tag)), timeoutMillis)) {
            error("Timed out waiting for Compose test tag to disappear: $tag")
        }
    }

    private fun UiObject2.dragToCenter() {
        val bounds = visibleBounds
        val y = bounds.centerY()
        drag(Point(bounds.centerX(), y), 16)
    }
}
