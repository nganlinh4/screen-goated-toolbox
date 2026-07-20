package dev.screengoated.toolbox.mobile.phonecontrol

import android.app.UiAutomation
import android.content.Intent
import android.os.ParcelFileDescriptor
import android.provider.Settings
import android.view.View
import android.view.WindowManager
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import dev.screengoated.toolbox.mobile.phonecontrol.overlay.PhoneControlOverlayController
import dev.screengoated.toolbox.mobile.phonecontrol.overlay.PhoneControlOverlayExclusion
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlRuntimeCode
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlRuntimePhase
import dev.screengoated.toolbox.mobile.phonecontrol.ui.PhoneControlPowerPreferences
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class PhoneControlOverlayExclusionTest {
    @Test
    fun captureLeaseHidesAndRestoresTheRenderedOverlay() = runBlocking {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val targetContext = instrumentation.targetContext
        val packageName = targetContext.packageName
        val originalMode = readOverlayMode(packageName)
        val originalPowerChoice = PhoneControlPowerPreferences.current(targetContext)
        PhoneControlPowerPreferences.clear(targetContext)
        val controller = PhoneControlOverlayController(targetContext)
        PhoneControlOverlayExclusion.register(controller)

        try {
            setOverlayMode(packageName, "allow")
            awaitCondition("Overlay permission did not become ready") {
                Settings.canDrawOverlays(targetContext)
            }
            val intent = Intent(
                targetContext,
                PhoneControlAccessibilityFixtureActivity::class.java,
            ).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            ActivityScenario.launch<PhoneControlAccessibilityFixtureActivity>(intent).use {
                controller.onState(
                    PhoneControlServiceState(
                        running = true,
                        phase = PhoneControlRuntimePhase.WORKING,
                        code = PhoneControlRuntimeCode.READY,
                        userMessage = "Working",
                        outputCaption = "Overlay exclusion fixture",
                    ),
                )
                awaitCondition("Phone Control orb never became visible") {
                    controller.orbBounds() != null
                }
                instrumentation.waitForIdleSync()
                delay(RENDER_SETTLE_MS)

                assertNotNull(controller.orbBounds())
                val rendererAlpha = windowParams(controller, "orbParams").alpha
                assertTrue(rendererAlpha > 0f)
                assertOverlaySuppression(controller, hidden = false, rendererAlpha)
                assertPowerPromptCanConsumeTouches(controller)

                PhoneControlOverlayExclusion.forCapture {
                    assertNull(controller.orbBounds())
                    assertOverlaySuppression(controller, hidden = true, rendererAlpha)
                }
                assertNotNull(controller.orbBounds())
                assertOverlaySuppression(controller, hidden = false, rendererAlpha)
                assertRendererWindowCannotConsumeTouches(controller)
            }
        } finally {
            PhoneControlOverlayExclusion.unregister(controller)
            withContext(Dispatchers.Main) { controller.destroy() }
            setOverlayMode(packageName, originalMode)
            if (originalPowerChoice == null) {
                PhoneControlPowerPreferences.clear(targetContext)
            } else {
                PhoneControlPowerPreferences.save(targetContext, originalPowerChoice)
            }
        }
    }

    private suspend fun awaitCondition(message: String, condition: () -> Boolean) {
        repeat(CONDITION_ATTEMPTS) {
            if (condition()) return
            delay(POLL_INTERVAL_MS)
        }
        error(message)
    }

    private fun assertRendererWindowCannotConsumeTouches(
        controller: PhoneControlOverlayController,
    ) {
        val params = windowParams(controller, "orbParams")
        assertTrue(
            "Visual renderer must remain non-touchable",
            params.flags and WindowManager.LayoutParams.FLAG_NOT_TOUCHABLE != 0,
        )
    }

    private fun assertOverlaySuppression(
        controller: PhoneControlOverlayController,
        hidden: Boolean,
        rendererAlpha: Float,
    ) {
        val expectedControlAlpha = if (hidden) 0f else 1f
        listOf("touchTarget", "powerPrompt").forEach { fieldName ->
            val field = PhoneControlOverlayController::class.java.getDeclaredField(fieldName)
            field.isAccessible = true
            val view = field.get(controller) as View
            assertEquals("Unexpected $fieldName alpha", expectedControlAlpha, view.alpha, 0f)
        }
        assertEquals(
            "Renderer readiness must not be reset by capture suppression",
            1f,
            view(controller, "orb").alpha,
            0f,
        )
        val expectedRendererAlpha = if (hidden) 0f else rendererAlpha
        assertEquals(expectedRendererAlpha, windowParams(controller, "orbParams").alpha, 0f)
        listOf("touchParams", "powerPromptParams").forEach { fieldName ->
            assertEquals(
                "Unexpected window alpha for $fieldName",
                expectedControlAlpha,
                windowParams(controller, fieldName).alpha,
                0f,
            )
        }
        val params = windowParams(controller, "touchParams")
        val notTouchable = params.flags and WindowManager.LayoutParams.FLAG_NOT_TOUCHABLE != 0
        assertEquals("Unexpected touch-target suppression", hidden, notTouchable)
    }

    private fun assertPowerPromptCanConsumeTouches(
        controller: PhoneControlOverlayController,
    ) {
        val params = windowParams(controller, "powerPromptParams")
        assertTrue(
            "Visible power prompt must accept a user choice",
            params.flags and WindowManager.LayoutParams.FLAG_NOT_TOUCHABLE == 0,
        )
    }

    private fun view(controller: PhoneControlOverlayController, name: String): View {
        val field = PhoneControlOverlayController::class.java.getDeclaredField(name)
        field.isAccessible = true
        return field.get(controller) as View
    }

    private fun windowParams(
        controller: PhoneControlOverlayController,
        name: String,
    ): WindowManager.LayoutParams {
        val field = PhoneControlOverlayController::class.java.getDeclaredField(name)
        field.isAccessible = true
        return field.get(controller) as WindowManager.LayoutParams
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
            .use { reader -> reader.readText() }
    }

    private companion object {
        val APP_OP_MODE = Regex("SYSTEM_ALERT_WINDOW:\\s+(allow|deny|ignore|default|foreground)")
        const val POLL_INTERVAL_MS = 100L
        const val CONDITION_ATTEMPTS = 50
        const val RENDER_SETTLE_MS = 250L
    }
}
