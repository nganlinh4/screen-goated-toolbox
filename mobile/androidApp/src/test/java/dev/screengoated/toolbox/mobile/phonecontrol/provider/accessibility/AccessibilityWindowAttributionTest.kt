package dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility

import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class AccessibilityWindowAttributionTest {
    @After
    fun clearStore() {
        AccessibilityWindowAttribution.clear()
    }

    @Test
    fun exactWindowAttributionIsGenerationBound() {
        AccessibilityWindowAttribution.record(41, "example.app", 7)

        assertEquals("example.app", AccessibilityWindowAttribution.resolve(41, 7))
        assertNull(AccessibilityWindowAttribution.resolve(42, 7))
        assertNull(AccessibilityWindowAttribution.resolve(41, 8))
    }

    @Test
    fun newerGenerationRetiresEveryOlderWindowAttribution() {
        AccessibilityWindowAttribution.record(41, "first.app", 7)
        AccessibilityWindowAttribution.record(42, "second.app", 8)

        assertNull(AccessibilityWindowAttribution.resolve(41, 7))
        assertEquals("second.app", AccessibilityWindowAttribution.resolve(42, 8))
    }

    @Test
    fun unknownOrInvalidEventIdentityIsNeverAttributed() {
        AccessibilityWindowAttribution.record(-1, "example.app", 7)
        AccessibilityWindowAttribution.record(41, " ", 7)
        AccessibilityWindowAttribution.record(41, "example.app", 0)

        assertNull(AccessibilityWindowAttribution.resolve(41, 7))
    }
}
