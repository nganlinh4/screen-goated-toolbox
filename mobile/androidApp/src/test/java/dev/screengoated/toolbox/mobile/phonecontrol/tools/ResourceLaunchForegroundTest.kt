package dev.screengoated.toolbox.mobile.phonecontrol.tools

import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityObservation
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityWindowSnapshot
import dev.screengoated.toolbox.mobile.phonecontrol.result.TargetBounds
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ResourceLaunchForegroundTest {
    @Test
    fun `exact active focused application package is preserved`() {
        val observation = observation(window(PACKAGE, active = true, focused = true))

        assertTrue(shouldPreserveForegroundLaunch(PACKAGE, observation))
    }

    @Test
    fun `other ambiguous and controller surfaces continue through launcher`() {
        assertFalse(
            shouldPreserveForegroundLaunch(
                PACKAGE,
                observation(window("other.package", active = true, focused = true)),
            ),
        )
        assertFalse(
            shouldPreserveForegroundLaunch(
                PACKAGE,
                observation(
                    window(PACKAGE, active = true, focused = true),
                    window(PACKAGE, active = true, focused = true, id = 2),
                ),
            ),
        )
        assertFalse(
            shouldPreserveForegroundLaunch(
                PACKAGE,
                observation(
                    window(PACKAGE, active = true, focused = true, controllerOwned = true),
                ),
            ),
        )
    }

    private fun observation(vararg windows: AccessibilityWindowSnapshot) =
        AccessibilityObservation(
            generation = 7,
            observedAtMs = 10,
            displayRotation = 0,
            densityDpi = 420,
            windows = windows.toList(),
            elements = emptyList(),
            truncated = false,
        )

    private fun window(
        packageName: String,
        active: Boolean,
        focused: Boolean,
        id: Int = 1,
        controllerOwned: Boolean = false,
    ) = AccessibilityWindowSnapshot(
        id = id,
        displayId = 0,
        layer = 0,
        type = APPLICATION_WINDOW,
        title = null,
        packageName = packageName,
        active = active,
        focused = focused,
        bounds = TargetBounds(0, 0, 100, 100),
        controllerOwned = controllerOwned,
    )

    private companion object {
        const val PACKAGE = "example.package"
    }
}
