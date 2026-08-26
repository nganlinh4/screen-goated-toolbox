package dev.screengoated.toolbox.mobile.baselineprofile

import androidx.benchmark.macro.MacrobenchmarkScope
import androidx.test.uiautomator.By
import androidx.test.uiautomator.Until

internal const val TARGET_PACKAGE = "dev.screengoated.toolbox.mobile"
private const val READY_TIMEOUT_MS = 10_000L

internal fun MacrobenchmarkScope.launchAndAwaitReady() {
    startActivityAndWait()
    check(device.wait(Until.hasObject(By.res("sgt-app-root")), READY_TIMEOUT_MS)) {
        "SGT root surface did not become ready"
    }
}

internal fun MacrobenchmarkScope.runCriticalUserJourney() {
    launchAndAwaitReady()
    val centerX = device.displayWidth / 2
    val upperY = device.displayHeight / 4
    val lowerY = device.displayHeight * 3 / 4
    device.swipe(centerX, lowerY, centerX, upperY, 12)
    device.waitForIdle()
    device.swipe(centerX, upperY, centerX, lowerY, 12)
    device.waitForIdle()
}
