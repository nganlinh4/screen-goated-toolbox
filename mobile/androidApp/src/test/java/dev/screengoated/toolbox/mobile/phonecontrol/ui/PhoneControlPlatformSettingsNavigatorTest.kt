package dev.screengoated.toolbox.mobile.phonecontrol.ui

import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityElement
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityObservation
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityTargetAuthority
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityWindowSnapshot
import dev.screengoated.toolbox.mobile.phonecontrol.result.PhoneControlTargetIdentity
import dev.screengoated.toolbox.mobile.phonecontrol.result.TargetBounds
import org.junit.Assert.assertEquals
import org.junit.Test

class PhoneControlPlatformSettingsNavigatorTest {
    @Test
    fun `unique runtime app label opens its row`() {
        val observation = observation(
            element(id = 1, label = "Another app"),
            element(id = 2, label = "SGT Mobile Debug"),
            element(id = 3, role = "scroll_container", actions = setOf("scroll_forward")),
        )

        assertEquals(
            PlatformSettingsNavigationDecision.OpenRow(2),
            choosePlatformSettingsNavigation(observation, SETTINGS_PACKAGE, "SGT Mobile Debug"),
        )
    }

    @Test
    fun `duplicate labels stop instead of guessing`() {
        val observation = observation(
            element(id = 1, label = "SGT Mobile"),
            element(id = 2, label = "SGT Mobile"),
        )

        assertEquals(
            PlatformSettingsNavigationDecision.AmbiguousTarget,
            choosePlatformSettingsNavigation(observation, SETTINGS_PACKAGE, "SGT Mobile"),
        )
    }

    @Test
    fun `missing row scrolls only the active settings surface`() {
        val observation = observation(
            element(
                id = 4,
                role = "scroll_container",
                actions = setOf("scroll_forward"),
                bounds = TargetBounds(0, 300, 1080, 2200),
            ),
            element(
                id = 5,
                role = "scroll_container",
                actions = setOf("scroll_forward"),
                bounds = TargetBounds(0, 300, 500, 1000),
            ),
        )

        assertEquals(
            PlatformSettingsNavigationDecision.ScrollForward(4),
            choosePlatformSettingsNavigation(observation, SETTINGS_PACKAGE, "SGT Mobile"),
        )
    }

    @Test
    fun `foreign foreground surface is never navigated`() {
        val observation = observation(
            element(id = 1, label = "SGT Mobile"),
            windowPackage = "com.example.foreground",
        )

        assertEquals(
            PlatformSettingsNavigationDecision.WaitForSurface,
            choosePlatformSettingsNavigation(observation, SETTINGS_PACKAGE, "SGT Mobile"),
        )
    }

    private fun observation(
        vararg elements: AccessibilityElement,
        windowPackage: String = SETTINGS_PACKAGE,
    ) = AccessibilityObservation(
        generation = 7,
        observedAtMs = 10,
        displayRotation = 0,
        densityDpi = 420,
        windows = listOf(
            AccessibilityWindowSnapshot(
                id = WINDOW_ID,
                displayId = 0,
                layer = 3,
                type = "application",
                title = null,
                packageName = windowPackage,
                active = true,
                focused = true,
                bounds = TargetBounds(0, 0, 1080, 2400),
                targetAuthority = AccessibilityTargetAuthority.OS_OWNED_USER_STEP,
            ),
        ),
        elements = elements.toList(),
        truncated = false,
    )

    private fun element(
        id: Int,
        label: String? = null,
        role: String = "textview",
        actions: Set<String> = emptySet(),
        bounds: TargetBounds = TargetBounds(100, 400, 900, 500),
    ) = AccessibilityElement(
        id = id,
        role = role,
        label = label,
        value = null,
        hint = null,
        stateDescription = null,
        viewId = null,
        packageName = SETTINGS_PACKAGE,
        className = null,
        bounds = bounds,
        actions = actions,
        enabled = true,
        visible = true,
        focused = false,
        selected = false,
        checked = null,
        controllerOwned = false,
        target = PhoneControlTargetIdentity(
            snapshotGeneration = 7,
            displayId = 0,
            windowId = WINDOW_ID.toLong(),
            packageOrSurface = SETTINGS_PACKAGE,
            nodeOrDocumentIdentity = "node-$id",
            bounds = bounds,
            observationTimestampMs = 10,
        ),
        targetAuthority = AccessibilityTargetAuthority.OS_OWNED_USER_STEP,
    )

    private companion object {
        const val SETTINGS_PACKAGE = "android.settings.owner"
        const val WINDOW_ID = 12
    }
}
