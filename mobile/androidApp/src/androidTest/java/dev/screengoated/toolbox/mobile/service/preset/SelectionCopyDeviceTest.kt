package dev.screengoated.toolbox.mobile.service.preset

import android.app.UiAutomation
import android.content.ComponentName
import android.content.Intent
import android.net.Uri
import android.os.SystemClock
import android.os.ParcelFileDescriptor
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.By
import androidx.test.uiautomator.Configurator
import androidx.test.uiautomator.UiDevice
import androidx.test.uiautomator.Until
import dev.screengoated.toolbox.mobile.service.SgtAccessibilityService
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class SelectionCopyDeviceTest {
    private fun connectedService(): SgtAccessibilityService {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        Configurator.getInstance().uiAutomationFlags = UiAutomation.FLAG_DONT_SUPPRESS_ACCESSIBILITY_SERVICES
        val automation = instrumentation.getUiAutomation(UiAutomation.FLAG_DONT_SUPPRESS_ACCESSIBILITY_SERVICES)
        fun shell(command: String) = automation.executeShellCommand(command).use {
            ParcelFileDescriptor.AutoCloseInputStream(it).bufferedReader().readText().trim()
        }
        val component = ComponentName(instrumentation.targetContext, SgtAccessibilityService::class.java).flattenToString()
        val services = shell("settings get secure enabled_accessibility_services")
        require(services.split(':').contains(component)) { "Enable SGT Accessibility first" }
        // Instrumentation force-stops its target; rebind only the already-consented service.
        shell("settings put secure enabled_accessibility_services ${services.split(':').filterNot { it == component }.joinToString(":").ifEmpty { "null" }}")
        shell("settings put secure enabled_accessibility_services $services")
        val deadline = SystemClock.elapsedRealtime() + 15_000
        while (SgtAccessibilityService.instance == null && SystemClock.elapsedRealtime() < deadline) {
            SystemClock.sleep(100)
        }
        return requireNotNull(SgtAccessibilityService.instance) { "Enable SGT Accessibility first" }
    }

    @Test fun pageCopyButtonIsNotASelectionActionAndWebViewSelectionStillCopies() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val service = connectedService()
        val device = UiDevice.getInstance(instrumentation)
        instrumentation.context.startActivity(Intent().setComponent(ComponentName(
            instrumentation.context.packageName, SelectionWebViewActivity::class.java.name,
        )).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        try {
            assertTrue(device.wait(Until.hasObject(By.text("Copy")), 15_000))
            SystemClock.sleep(1000)
            instrumentation.runOnMainSync { service.copyToClipboard("Unselected") }
            instrumentation.runOnMainSync { service.eagerCaptureSelection() }
            SystemClock.sleep(500)
            assertTrue("Native button must not be invoked", device.hasObject(By.text("Native actions:0")))
            assertTrue("Page button must not be invoked", device.hasObject(By.text("Page actions:0")))
            val bounds = requireNotNull(device.findObject(By.text("Lantern"))).visibleBounds
            device.swipe(bounds.left + 20, bounds.centerY(), bounds.left + 20, bounds.centerY(), 150)
            SystemClock.sleep(500)
            device.takeScreenshot(File(instrumentation.targetContext.cacheDir, "selection-before.png"))
            device.dumpWindowHierarchy(File(instrumentation.targetContext.cacheDir, "selection-before.xml"))
            instrumentation.runOnMainSync {
                fun describe(node: android.view.accessibility.AccessibilityNodeInfo, lines: MutableList<String>) {
                    lines += "${node.className} id=${node.viewIdResourceName} focus=${node.isFocused} selected=${node.textSelectionStart}:${node.textSelectionEnd} actions=${node.actionList.map { it.id }}"
                    for (i in 0 until node.childCount) node.getChild(i)?.let { describe(it, lines) }
                }
                val lines = mutableListOf<String>()
                service.windows.forEach { it.root?.takeIf { root -> root.packageName?.toString() == instrumentation.context.packageName }?.let { root -> describe(root, lines) } }
                File(instrumentation.targetContext.cacheDir, "selection-actions.txt").writeText(lines.joinToString("\n"))
            }
            instrumentation.runOnMainSync { service.eagerCaptureSelection() }
            val copied = CountDownLatch(1)
            var text: String? = null
            instrumentation.runOnMainSync { service.readClipboardAsync { text = it; copied.countDown() } }
            assertTrue(copied.await(5, TimeUnit.SECONDS))
            assertEquals("Lantern", text)
        } finally {
            device.pressBack()
        }
    }

    @Test fun browserSelectionStillCopies() {
        val url = InstrumentationRegistry.getArguments().getString("selectionChromeUrl")
        org.junit.Assume.assumeTrue("Provide the public fixture URL for browser acceptance", url != null)
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val service = connectedService()
        val device = UiDevice.getInstance(instrumentation)
        instrumentation.targetContext.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url))
            .setPackage("com.android.chrome").addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        try {
            assertTrue(device.wait(Until.hasObject(By.text("Lantern")), 20_000))
            instrumentation.runOnMainSync { service.copyToClipboard("Unselected") }
            val bounds = requireNotNull(device.findObject(By.text("Lantern"))).visibleBounds
            device.swipe(bounds.left + 20, bounds.centerY(), bounds.left + 20, bounds.centerY(), 150)
            SystemClock.sleep(700)
            device.takeScreenshot(File(instrumentation.targetContext.cacheDir, "browser-selection-before.png"))
            instrumentation.runOnMainSync { service.eagerCaptureSelection() }
            val copied = CountDownLatch(1)
            var text: String? = null
            instrumentation.runOnMainSync { service.readClipboardAsync { text = it; copied.countDown() } }
            assertTrue(copied.await(5, TimeUnit.SECONDS))
            assertEquals("Lantern", text)
        } finally {
            device.pressBack()
        }
    }
}
