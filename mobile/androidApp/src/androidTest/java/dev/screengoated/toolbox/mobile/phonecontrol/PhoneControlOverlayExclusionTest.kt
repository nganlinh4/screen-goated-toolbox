package dev.screengoated.toolbox.mobile.phonecontrol

import android.app.UiAutomation
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.Color
import android.graphics.Rect
import android.os.ParcelFileDescriptor
import android.provider.Settings
import android.view.WindowManager
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.UiDevice
import dev.screengoated.toolbox.mobile.phonecontrol.overlay.PhoneControlOverlayController
import dev.screengoated.toolbox.mobile.phonecontrol.overlay.PhoneControlOverlayExclusion
import dev.screengoated.toolbox.mobile.phonecontrol.overlay.maskPhoneControlOverlay
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlRuntimeCode
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlRuntimePhase
import dev.screengoated.toolbox.mobile.phonecontrol.ui.PhoneControlPowerPreferences
import dev.screengoated.toolbox.mobile.phonecontrol.ui.PhoneControlPowerChoice
import dev.screengoated.toolbox.mobile.service.DismissAction
import dev.screengoated.toolbox.mobile.service.DismissBubbleController
import java.util.concurrent.atomic.AtomicBoolean
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class PhoneControlOverlayExclusionTest {
    @Test
    fun draggingOrbIntoSharedDismissBubbleCommitsOneServiceStop() = runBlocking {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val targetContext = instrumentation.targetContext
        val packageName = targetContext.packageName
        val originalMode = readOverlayMode(packageName)
        val originalPowerChoice = PhoneControlPowerPreferences.current(targetContext)
        val dismissed = AtomicBoolean(false)
        PhoneControlPowerPreferences.save(targetContext, PhoneControlPowerChoice.STANDARD)
        val controller = PhoneControlOverlayController(targetContext, onDismiss = {
            assertTrue("Dismiss callback must run once", dismissed.compareAndSet(false, true))
        })

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
                        authorityGuidance = "Setting up",
                    ),
                )
                awaitCondition("Phone Control orb never became visible") {
                    controller.orbBounds() != null
                }
                instrumentation.waitForIdleSync()

                val device = UiDevice.getInstance(instrumentation)
                requireNotNull(controller.orbBounds())
                val touchLayout = windowParams(controller, "touchParams")
                val screen = Rect(0, 0, device.displayWidth, device.displayHeight)
                val dismiss = dismissController(controller)
                assertEquals(
                    "Dismiss target must share the orb renderer window layer",
                    windowParams(controller, "orbParams").type,
                    configuredDismissWindowType(dismiss),
                )
                val target = requireNotNull(
                    dismiss.targetCenterPx(DismissAction.SINGLE, screen),
                )
                assertTrue(
                    "UiAutomator could not inject the orb dismiss drag",
                    device.swipe(
                        touchLayout.x + touchLayout.width / 2,
                        touchLayout.y + touchLayout.height / 2,
                        target.first.toInt(),
                        target.second.toInt(),
                        DISMISS_SWIPE_STEPS,
                    ),
                )
                awaitCondition("Orb dismiss did not request service stop") { dismissed.get() }
            }
        } finally {
            withContext(Dispatchers.Main) { controller.destroy() }
            setOverlayMode(packageName, originalMode)
            if (originalPowerChoice == null) {
                PhoneControlPowerPreferences.clear(targetContext)
            } else {
                PhoneControlPowerPreferences.save(targetContext, originalPowerChoice)
            }
        }
    }

    @Test
    fun captureBoundsDoNotMutateTheRenderedOverlay() = runBlocking {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val targetContext = instrumentation.targetContext
        val packageName = targetContext.packageName
        val originalMode = readOverlayMode(packageName)
        val originalPowerChoice = PhoneControlPowerPreferences.current(targetContext)
        PhoneControlPowerPreferences.clear(targetContext)
        val controller = PhoneControlOverlayController(targetContext, onDismiss = {})
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
                assertPowerPromptCanConsumeTouches(controller)

                val captureBounds = requireNotNull(
                    PhoneControlOverlayExclusion.currentCaptureBounds(),
                )
                val device = UiDevice.getInstance(instrumentation)
                repeat(CAPTURE_MASK_REPETITIONS) {
                    val frame = Bitmap.createBitmap(
                        device.displayWidth,
                        device.displayHeight,
                        Bitmap.Config.ARGB_8888,
                    ).apply { eraseColor(Color.WHITE) }
                    val masked = maskPhoneControlOverlay(frame)
                    assertSame("Mutable capture should be masked in place", frame, masked)
                    val centerX = ((captureBounds.left + captureBounds.right) / 2)
                        .coerceIn(0, masked.width - 1)
                    val centerY = ((captureBounds.top + captureBounds.bottom) / 2)
                        .coerceIn(0, masked.height - 1)
                    assertEquals(Color.BLACK, masked.getPixel(centerX, centerY))
                    assertEquals(rendererAlpha, windowParams(controller, "orbParams").alpha, 0f)
                    masked.recycle()
                }
                assertNotNull(controller.orbBounds())
                assertEquals(rendererAlpha, windowParams(controller, "orbParams").alpha, 0f)
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

    private fun assertPowerPromptCanConsumeTouches(
        controller: PhoneControlOverlayController,
    ) {
        val params = windowParams(controller, "powerPromptParams")
        assertTrue(
            "Visible power prompt must accept a user choice",
            params.flags and WindowManager.LayoutParams.FLAG_NOT_TOUCHABLE == 0,
        )
    }

    private fun windowParams(
        controller: PhoneControlOverlayController,
        name: String,
    ): WindowManager.LayoutParams {
        val field = PhoneControlOverlayController::class.java.getDeclaredField(name)
        field.isAccessible = true
        return field.get(controller) as WindowManager.LayoutParams
    }

    private fun dismissController(
        controller: PhoneControlOverlayController,
    ): DismissBubbleController {
        val field = PhoneControlOverlayController::class.java.getDeclaredField("dismissBubble")
        field.isAccessible = true
        return field.get(controller) as DismissBubbleController
    }

    private fun configuredDismissWindowType(controller: DismissBubbleController): Int {
        val field = DismissBubbleController::class.java.getDeclaredField("windowType")
        field.isAccessible = true
        return field.getInt(controller)
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
        const val CAPTURE_MASK_REPETITIONS = 4
        const val DISMISS_SWIPE_STEPS = 30
    }
}
