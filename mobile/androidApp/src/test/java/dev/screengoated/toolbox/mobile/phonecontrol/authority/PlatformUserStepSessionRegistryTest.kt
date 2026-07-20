package dev.screengoated.toolbox.mobile.phonecontrol.authority

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PlatformUserStepSessionRegistryTest {
    @Test
    fun slotCannotDuplicateAndRetiresExactlyOnce() {
        val baseline = PlatformUserStepSessionRegistry.snapshot().activeCount
        val slot = PlatformUserStepSlot()

        assertTrue(slot.begin())
        assertFalse(slot.begin())
        assertEquals(baseline + 1, PlatformUserStepSessionRegistry.snapshot().activeCount)
        assertTrue(slot.finish())
        assertFalse(slot.finish())
        assertEquals(baseline, PlatformUserStepSessionRegistry.snapshot().activeCount)
    }

    @Test
    fun multipleSessionsRemainActiveUntilEveryOwnerEnds() {
        val baseline = PlatformUserStepSessionRegistry.snapshot()
        val first = PlatformUserStepSessionRegistry.begin()
        val second = PlatformUserStepSessionRegistry.begin()
        try {
            val active = PlatformUserStepSessionRegistry.snapshot()
            assertTrue(active.active)
            assertEquals(baseline.activeCount + 2, active.activeCount)

            assertTrue(PlatformUserStepSessionRegistry.end(first))
            assertTrue(PlatformUserStepSessionRegistry.hasActiveSession())
            assertFalse(PlatformUserStepSessionRegistry.end(first))
        } finally {
            PlatformUserStepSessionRegistry.end(first)
            PlatformUserStepSessionRegistry.end(second)
        }
        assertEquals(baseline.activeCount, PlatformUserStepSessionRegistry.snapshot().activeCount)
    }
}
