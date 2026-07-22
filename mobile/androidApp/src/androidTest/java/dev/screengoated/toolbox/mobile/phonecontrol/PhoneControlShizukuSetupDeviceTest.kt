package dev.screengoated.toolbox.mobile.phonecontrol

import android.app.UiAutomation
import android.content.Intent
import android.net.Uri
import android.os.ParcelFileDescriptor
import android.provider.Settings
import android.view.ViewGroup
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import dev.screengoated.toolbox.mobile.phonecontrol.overlay.PhoneControlOverlayController
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlRuntimeCode
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlRuntimePhase
import dev.screengoated.toolbox.mobile.phonecontrol.ui.PhoneControlPowerChoice
import dev.screengoated.toolbox.mobile.phonecontrol.ui.PhoneControlPowerPreferences
import java.util.concurrent.atomic.AtomicBoolean
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import kotlinx.coroutines.Dispatchers
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeFalse
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class PhoneControlShizukuSetupDeviceTest {
    @Test
    fun missingShizukuSelectionStartsOfficialInstallFlow() {
        runBlocking {
            val instrumentation = InstrumentationRegistry.getInstrumentation()
            val context = instrumentation.targetContext
            assumeFalse("This device already has Shizuku", isPackageInstalled(context, SHIZUKU_PACKAGE))
            val originalMode = readOverlayMode(context.packageName)
            val originalChoice = PhoneControlPowerPreferences.current(context)
            val controller = PhoneControlOverlayController(context, onDismiss = {})
            val clicked = AtomicBoolean(false)

            try {
                setOverlayMode(context.packageName, "allow")
                awaitCondition("Overlay permission did not become ready") {
                    Settings.canDrawOverlays(context)
                }
                PhoneControlPowerPreferences.clear(context)
                requireNotNull(
                    installRoute(context).resolveActivity(context.packageManager),
                )
                val fixture = Intent(
                    context,
                    PhoneControlAccessibilityFixtureActivity::class.java,
                ).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                ActivityScenario.launch<PhoneControlAccessibilityFixtureActivity>(fixture).use {
                    controller.onState(
                        PhoneControlServiceState(
                            running = true,
                            phase = PhoneControlRuntimePhase.WORKING,
                            code = PhoneControlRuntimeCode.READY,
                            userMessage = "Working",
                        ),
                    )
                    awaitCondition("Phone Control power prompt never appeared") {
                        powerPrompt(controller) != null
                    }
                    instrumentation.runOnMainSync {
                        val prompt = requireNotNull(powerPrompt(controller))
                        val choices = prompt.getChildAt(2) as ViewGroup
                        clicked.set(choices.getChildAt(1).performClick())
                    }
                    assertTrue("Shizuku choice did not accept the click", clicked.get())
                    assertEquals(
                        PhoneControlPowerChoice.SHIZUKU,
                        PhoneControlPowerPreferences.current(context),
                    )
                }
            } finally {
                withContext(Dispatchers.Main) { controller.destroy() }
                setOverlayMode(context.packageName, originalMode)
                if (originalChoice == null) {
                    PhoneControlPowerPreferences.clear(context)
                } else {
                    PhoneControlPowerPreferences.save(context, originalChoice)
                }
            }
        }
    }

    private fun installRoute(context: android.content.Context): Intent {
        val store = Intent(
            Intent.ACTION_VIEW,
            Uri.parse("market://details?id=$SHIZUKU_PACKAGE"),
        ).setPackage(PLAY_STORE_PACKAGE)
        return store.takeIf { it.resolveActivity(context.packageManager) != null }
            ?: Intent(Intent.ACTION_VIEW, Uri.parse(SHIZUKU_DOWNLOAD_URL))
                .addCategory(Intent.CATEGORY_BROWSABLE)
    }

    private fun isPackageInstalled(context: android.content.Context, packageName: String): Boolean =
        runCatching { context.packageManager.getPackageInfo(packageName, 0) }.isSuccess

    private fun powerPrompt(controller: PhoneControlOverlayController): ViewGroup? {
        val field = PhoneControlOverlayController::class.java.getDeclaredField("powerPrompt")
        field.isAccessible = true
        return field.get(controller) as? ViewGroup
    }

    private suspend fun awaitCondition(message: String, condition: () -> Boolean) {
        repeat(CONDITION_ATTEMPTS) {
            if (condition()) return
            delay(POLL_INTERVAL_MS)
        }
        error(message)
    }

    private fun readOverlayMode(packageName: String): String {
        val output = shell("appops get $packageName SYSTEM_ALERT_WINDOW")
        return APP_OP_MODE.find(output)?.groupValues?.get(1) ?: "default"
    }

    private fun setOverlayMode(packageName: String, mode: String) {
        shell("appops set $packageName SYSTEM_ALERT_WINDOW $mode")
    }

    private fun shell(command: String): String {
        val descriptor = InstrumentationRegistry.getInstrumentation()
            .getUiAutomation(UiAutomation.FLAG_DONT_SUPPRESS_ACCESSIBILITY_SERVICES)
            .executeShellCommand(command)
        return ParcelFileDescriptor.AutoCloseInputStream(descriptor)
            .bufferedReader()
            .use { it.readText() }
    }

    private companion object {
        val APP_OP_MODE = Regex("SYSTEM_ALERT_WINDOW:\\s+(allow|deny|ignore|default|foreground)")
        const val SHIZUKU_PACKAGE = "moe.shizuku.privileged.api"
        const val PLAY_STORE_PACKAGE = "com.android.vending"
        const val SHIZUKU_DOWNLOAD_URL = "https://shizuku.rikka.app/download/"
        const val POLL_INTERVAL_MS = 100L
        const val CONDITION_ATTEMPTS = 80
    }
}
