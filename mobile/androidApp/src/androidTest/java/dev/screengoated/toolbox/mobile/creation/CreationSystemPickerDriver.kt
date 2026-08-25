package dev.screengoated.toolbox.mobile.creation

import android.content.Context
import android.graphics.Rect
import android.net.Uri
import android.os.SystemClock
import android.provider.MediaStore
import androidx.test.uiautomator.By
import androidx.test.uiautomator.StaleObjectException
import androidx.test.uiautomator.UiDevice
import androidx.test.uiautomator.Until
import java.util.regex.Pattern

internal class CreationSystemPickerDriver(
    private val context: Context,
    private val device: UiDevice,
) {
    fun select(uri: Uri) {
        val displayName = context.contentResolver.query(
            uri,
            arrayOf(MediaStore.Images.Media.DISPLAY_NAME),
            null,
            null,
            null,
        )!!.use { cursor ->
            check(cursor.moveToFirst())
            cursor.getString(0)
        }
        val bounds = clickTile(displayName)
        device.waitForIdle(1_000)
        confirmSelectionIfPresent()
        if (!waitForCreationAppForeground(3_000)) {
            val recoveredIntoApp = recoverUnresponsivePicker() &&
                waitForCreationAppForeground(5_000)
            if (!recoveredIntoApp) {
                clickTile(displayName)
                device.waitForIdle(1_000)
                confirmSelectionIfPresent()
            }
        }
        if (!waitForCreationAppForeground(10_000) && recoverUnresponsivePicker()) {
            waitForCreationAppForeground(10_000)
        }
        check(waitForCreationAppForeground(20_000)) {
            "Creation app did not regain focus after choosing the quality-control image; " +
                "foreground package=${device.currentPackageName}, tileBounds=$bounds"
        }
    }

    private fun waitForCreationAppForeground(timeoutMillis: Long): Boolean {
        val deadline = SystemClock.elapsedRealtime() + timeoutMillis
        while (SystemClock.elapsedRealtime() < deadline) {
            if (device.currentPackageName == context.packageName) return true
            SystemClock.sleep(100)
        }
        return device.currentPackageName == context.packageName
    }

    private fun clickTile(displayName: String): Rect {
        val description = By.desc(Pattern.compile("^${Pattern.quote(displayName)},.*"))
        val text = By.text(displayName)
        val deadline = SystemClock.elapsedRealtime() + 60_000
        while (SystemClock.elapsedRealtime() < deadline) {
            val target = device.findObject(description) ?: device.findObject(text)
            if (target != null) {
                try {
                    val bounds = target.visibleBounds
                    if (!bounds.isEmpty) {
                        target.longClick()
                        return bounds
                    }
                } catch (_: StaleObjectException) {
                    // The picker replaces thumbnail nodes while their previews are decoded.
                }
            }
            SystemClock.sleep(250)
        }
        error("System picker did not show a stable, visible quality-control image tile")
    }

    private fun confirmSelectionIfPresent() {
        val selectAction = device.wait(
            Until.findObject(By.res(SYSTEM_PICKER_PACKAGE, "action_menu_select")),
            5_000,
        )
        if (selectAction != null) {
            selectAction.click()
            device.waitForIdle(1_000)
            return
        }
        val labels = listOf("Open", "Select", "Add", "Done")
        labels.firstNotNullOfOrNull { label ->
            device.wait(Until.findObject(By.text(label)), 2_000)
        }?.click()
        device.waitForIdle(1_000)
    }

    private fun recoverUnresponsivePicker(): Boolean {
        val wait = device.wait(Until.findObject(By.res("android", "aerr_wait")), 3_000)
            ?: return false
        wait.click()
        device.waitForIdle(2_000)
        return true
    }

    private companion object {
        const val SYSTEM_PICKER_PACKAGE = "com.google.android.documentsui"
    }
}
