package dev.screengoated.toolbox.mobile.phonecontrol.provider.detector

import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityObservation
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityWindowSnapshot
import dev.screengoated.toolbox.mobile.phonecontrol.result.TargetBounds
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class UiDetectorSurfaceTest {
    @Test
    fun selectsRootlessActiveApplicationSurface() {
        val rootless = window(id = 4, contentAccessible = false, active = true)

        assertEquals(rootless, detectorSurface(observation(rootless)))
    }

    @Test
    fun excludesControllerInputMethodAndSecondaryDisplaySurfaces() {
        val controller = window(id = 1, active = true, controllerOwned = true)
        val keyboard = window(id = 2, active = true, type = "input_method")
        val secondary = window(id = 3, active = true, displayId = 1)

        assertNull(detectorSurface(observation(controller, keyboard, secondary)))
    }

    private fun observation(vararg windows: AccessibilityWindowSnapshot) =
        AccessibilityObservation(
            generation = 9,
            observedAtMs = 10,
            displayRotation = 0,
            densityDpi = 420,
            windows = windows.toList(),
            elements = emptyList(),
            truncated = false,
        )

    private fun window(
        id: Int,
        contentAccessible: Boolean = true,
        active: Boolean = false,
        controllerOwned: Boolean = false,
        type: String = "application",
        displayId: Int = 0,
    ) = AccessibilityWindowSnapshot(
        id = id,
        displayId = displayId,
        layer = id,
        type = type,
        title = "surface-$id",
        packageName = "example.$id",
        active = active,
        focused = false,
        bounds = TargetBounds(0, 0, 100, 100),
        contentAccessible = contentAccessible,
        controllerOwned = controllerOwned,
    )
}
