package dev.screengoated.toolbox.mobile.phonecontrol.provider.visual

import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityWindowSnapshot
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.ACTIVE_CONTENT_WINDOW_TYPE
import dev.screengoated.toolbox.mobile.phonecontrol.result.TargetBounds
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class VisualSurfaceSelectionTest {
    @Test
    fun `controller overlay does not hide underlying external application`() {
        val external = window(id = 1, layer = 2, active = false, contentAccessible = false)
        val orb = window(
            id = 2,
            layer = 3,
            active = true,
            type = "accessibility_overlay",
            controllerOwned = true,
        )

        assertEquals(external, selectVisualSurface(listOf(external, orb)))
    }

    @Test
    fun `active external surface outranks higher inactive surface`() {
        val active = window(id = 3, layer = 1, active = true)
        val inactive = window(id = 4, layer = 9, active = false)

        assertEquals(active, selectVisualSurface(listOf(inactive, active)))
    }

    @Test
    fun `rootless external application remains visually shareable`() {
        val rootless = window(id = 5, contentAccessible = false)

        assertEquals(rootless, selectVisualSurface(listOf(rootless)))
    }

    @Test
    fun `synthesized active root remains visually shareable`() {
        val activeRoot = window(id = 6, type = ACTIVE_CONTENT_WINDOW_TYPE)

        assertEquals(activeRoot, selectVisualSurface(listOf(activeRoot)))
    }

    @Test
    fun `controller owned app alone has no shareable surface`() {
        assertNull(selectVisualSurface(listOf(window(id = 7, controllerOwned = true))))
    }

    private fun window(
        id: Int,
        layer: Int = 1,
        active: Boolean = true,
        contentAccessible: Boolean = true,
        type: String = "application",
        controllerOwned: Boolean = false,
    ) = AccessibilityWindowSnapshot(
        id = id,
        displayId = 0,
        layer = layer,
        type = type,
        title = "fixture-$id",
        packageName = "fixture.package.$id",
        active = active,
        focused = false,
        bounds = TargetBounds(0, 0, 100, 100),
        contentAccessible = contentAccessible,
        controllerOwned = controllerOwned,
    )
}
