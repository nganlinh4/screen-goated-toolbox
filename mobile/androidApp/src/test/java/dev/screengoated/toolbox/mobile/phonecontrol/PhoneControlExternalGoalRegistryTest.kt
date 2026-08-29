package dev.screengoated.toolbox.mobile.phonecontrol

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class PhoneControlExternalGoalRegistryTest {
    @Test
    fun `slot keeps one transient bounded goal`() {
        var now = 10L
        val slot = PhoneControlExternalGoalSlot(now = { now }, maximumAgeMs = 50L)

        assertTrue(slot.offer("  first  "))
        assertEquals("first", slot.peek())
        assertFalse(slot.offer("second"))
        assertFalse(slot.complete("different"))
        assertTrue(slot.complete("first"))
        assertNull(slot.peek())
        assertFalse(slot.offer("x".repeat(1_025)))

        assertTrue(slot.offer("expires"))
        now = 61L
        assertNull(slot.peek())
    }
}
