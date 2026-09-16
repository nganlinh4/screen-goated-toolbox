package dev.screengoated.toolbox.mobile.service.preset

import android.app.UiAutomation
import android.content.ComponentName
import android.content.Intent
import android.os.SystemClock
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.By
import androidx.test.uiautomator.Configurator
import androidx.test.uiautomator.UiDevice
import androidx.test.uiautomator.Until
import dev.screengoated.toolbox.mobile.service.SgtAccessibilityService
import java.util.concurrent.atomic.AtomicReference
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class DictationDestinationAcceptanceTest {
    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private val automation = instrumentation.getUiAutomation(UiAutomation.FLAG_DONT_SUPPRESS_ACCESSIBILITY_SERVICES)
    private fun shell(command: String): String = automation.executeShellCommand(command).use {
        android.os.ParcelFileDescriptor.AutoCloseInputStream(it).bufferedReader().readText().trim()
    }
    private fun <T> main(action: () -> T): T {
        val result = AtomicReference<Result<T>>()
        instrumentation.runOnMainSync { result.set(runCatching(action)) }
        return result.get().getOrThrow()
    }

    @Test fun focusHandoffKeepsCommittedTextAndPreservesUserEdits() {
        val context = instrumentation.targetContext
        val component = ComponentName(context, SgtAccessibilityService::class.java).flattenToString()
        val previousServices = shell("settings get secure enabled_accessibility_services")
        val previousEnabled = shell("settings get secure accessibility_enabled")
        require(previousServices == "null" || previousServices.matches(Regex("[A-Za-z0-9_./:]*")))
        require(previousEnabled in setOf("null", "0", "1"))
        val services = previousServices.takeUnless { it == "null" }.orEmpty()
            .split(':').filter(String::isNotBlank).plus(component).distinct().joinToString(":")
        var route: RebindingPasteSession? = null
        val configurator = Configurator.getInstance()
        val previousFlags = configurator.uiAutomationFlags
        try {
            configurator.uiAutomationFlags = UiAutomation.FLAG_DONT_SUPPRESS_ACCESSIBILITY_SERVICES
            shell("settings put secure enabled_accessibility_services $services")
            assertEquals(services, shell("settings get secure enabled_accessibility_services"))
            shell("settings put secure accessibility_enabled 1")
            val deadline = SystemClock.elapsedRealtime() + 15_000
            while (SgtAccessibilityService.instance == null && SystemClock.elapsedRealtime() < deadline) {
                SystemClock.sleep(100)
            }
            assertTrue("Accessibility service did not connect", SgtAccessibilityService.instance != null)
            val device = UiDevice.getInstance(instrumentation)
            instrumentation.context.startActivity(Intent().setComponent(ComponentName(
                instrumentation.context.packageName, DictationEditorActivity::class.java.name,
            )).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
            val first = requireNotNull(device.wait(Until.findObject(By.desc("First editor")), 15_000))
            val second = requireNotNull(device.wait(Until.findObject(By.desc("Second editor")), 15_000))
            first.click()
            SystemClock.sleep(500)
            val session = main {
                val service = requireNotNull(SgtAccessibilityService.instance) { "Service disconnected during editor setup" }
                val target = requireNotNull(AccessibilityProvisionalPasteTarget.capture(service)) {
                    "No eligible destination: " + service.windows.joinToString { window ->
                        val node = window.root?.findFocus(android.view.accessibility.AccessibilityNodeInfo.FOCUS_INPUT)
                        "type=${window.type} focused=${window.isFocused} active=${window.isActive} " +
                            "node=${node != null} editable=${node?.isEditable} visible=${node?.isVisibleToUser} " +
                            "selection=${node?.textSelectionStart}:${node?.textSelectionEnd} " +
                            "actions=${node?.actionList?.map { it.id }}"
                    }
                }
                RebindingPasteSession(ProvisionalPasteSession(target), {
                    AccessibilityProvisionalPasteTarget.capture(SgtAccessibilityService.instance)
                })
            }
            route = session
            assertTrue(main { session.deliver(ProvisionalPasteEvent.Interim("first words")) })
            second.click()
            SystemClock.sleep(250)
            val next = ProvisionalPasteEvent.Final("first words continue")
            assertFalse(main { session.deliver(next) })
            SystemClock.sleep(600)
            assertTrue(main { session.deliver(next) })
            assertEquals("first words", first.text)
            assertEquals(" continue", second.text)
            assertTrue(main { session.deliver(ProvisionalPasteEvent.Interim(" temporary")) })
            second.text = "user edit"
            main { session.close() }
            assertEquals("user edit", second.text)
            assertFalse(main { session.deliver(ProvisionalPasteEvent.Final("late")) })
        } finally {
            main { route?.close() }
            if (previousServices == "null") shell("settings delete secure enabled_accessibility_services")
            else if (previousServices.isEmpty()) shell("settings delete secure enabled_accessibility_services")
            else shell("settings put secure enabled_accessibility_services $previousServices")
            if (previousEnabled == "null") shell("settings delete secure accessibility_enabled")
            else shell("settings put secure accessibility_enabled $previousEnabled")
            configurator.uiAutomationFlags = previousFlags
        }
    }
}
