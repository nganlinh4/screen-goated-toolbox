package dev.screengoated.toolbox.mobile.service.preset

import android.app.UiAutomation
import android.content.ComponentName
import android.content.Intent
import android.os.Bundle
import android.os.SystemClock
import android.view.accessibility.AccessibilityNodeInfo
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.uiautomator.By
import androidx.test.uiautomator.Configurator
import androidx.test.uiautomator.Until
import dev.screengoated.toolbox.mobile.service.SgtAccessibilityService
import dev.screengoated.toolbox.mobile.shared.preset.PresetType
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class ContinuousSelectionUiAcceptanceTest {
    @Test fun selectionCaptureRepeatsAndStopDisarmsIt() = PresetUiAcceptanceSupport().use { ui ->
        val component = ComponentName(ui.context, SgtAccessibilityService::class.java).flattenToString()
        val services = ui.shell("settings get secure enabled_accessibility_services")
        val enabled = ui.shell("settings get secure accessibility_enabled")
        require(services == "null" || services.matches(Regex("[A-Za-z0-9_./:]*")))
        require(enabled in setOf("null", "0", "1"))
        val flags = Configurator.getInstance().uiAutomationFlags
        try {
            Configurator.getInstance().uiAutomationFlags = UiAutomation.FLAG_DONT_SUPPRESS_ACCESSIBILITY_SERVICES
            val requested = services.takeUnless { it == "null" }.orEmpty().split(':')
                .filter(String::isNotBlank).plus(component).distinct().joinToString(":")
            ui.shell("settings put secure enabled_accessibility_services $requested")
            ui.shell("settings put secure accessibility_enabled 1")
            ui.await("accessibility connected") { SgtAccessibilityService.instance != null }
            val id = ui.preset(PresetType.TEXT_SELECT)
            ui.instrumentation.context.startActivity(Intent().setComponent(ComponentName(
                ui.instrumentation.context.packageName, DictationEditorActivity::class.java.name,
            )).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
            val editor = requireNotNull(ui.device.wait(Until.findObject(By.desc("First editor")), 15_000))
            ui.main { ui.controller.launchPreset(id, closePanel = true, continuousMode = true) }
            for (text in listOf("First selection", "Second selection")) {
                editor.text = text
                // The floating selection badge overlaps the editor's center.
                val editorBounds = editor.visibleBounds
                ui.device.click(editorBounds.left + 12, editorBounds.centerY())
                ui.await("editor focus and selection") { ui.main {
                    val focus = SgtAccessibilityService.instance?.rootInActiveWindow
                        ?.findFocus(AccessibilityNodeInfo.FOCUS_INPUT)
                    focus?.performAction(AccessibilityNodeInfo.ACTION_SET_SELECTION, Bundle().apply {
                        putInt(AccessibilityNodeInfo.ACTION_ARGUMENT_SELECTION_START_INT, 0)
                        putInt(AccessibilityNodeInfo.ACTION_ARGUMENT_SELECTION_END_INT, text.length)
                    }) == true
                } }
                ui.textControl("Select text (Continuous)").click()
                ui.await("captured $text") {
                    ui.repository.executionState.value.resultWindows.any { it.markdownText == text }
                }
                ui.main { ui.controller.resultModule.destroy() }
                assertEquals("Selection stays armed after capture", id, ui.main { ui.controller.continuousSelection.presetId })
            }
            ui.textControl("Stop").click()
            ui.await("selection disarmed") { ui.main { ui.controller.continuousSelection.presetId == null } }
            val completed = ui.repository.executionState.value.sessionId
            editor.text = "After stop"
            SystemClock.sleep(700)
            assertEquals(completed, ui.repository.executionState.value.sessionId)
            assertFalse(ui.device.hasObject(By.text("Select text (Continuous)")))
        } finally {
            if (services == "null" || services.isEmpty()) ui.shell("settings delete secure enabled_accessibility_services")
            else ui.shell("settings put secure enabled_accessibility_services $services")
            if (enabled == "null") ui.shell("settings delete secure accessibility_enabled")
            else ui.shell("settings put secure accessibility_enabled $enabled")
            Configurator.getInstance().uiAutomationFlags = flags
        }
    }
}
