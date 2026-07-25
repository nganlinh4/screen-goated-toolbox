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
        val first = PlatformUserStepSessionRegistry.begin(setOf("fixture.first"))
        val second = PlatformUserStepSessionRegistry.begin(setOf("fixture.second"))
        try {
            val active = PlatformUserStepSessionRegistry.snapshot()
            assertTrue(active.active)
            assertEquals(baseline.activeCount + 2, active.activeCount)
            assertTrue("fixture.first" in active.expectedPackageNames)
            assertTrue("fixture.second" in active.expectedPackageNames)

            assertTrue(PlatformUserStepSessionRegistry.end(first))
            assertTrue(PlatformUserStepSessionRegistry.hasActiveSession())
            assertFalse(PlatformUserStepSessionRegistry.end(first))
            assertFalse(
                "fixture.first" in PlatformUserStepSessionRegistry.snapshot().expectedPackageNames,
            )
        } finally {
            PlatformUserStepSessionRegistry.end(first)
            PlatformUserStepSessionRegistry.end(second)
        }
        val restored = PlatformUserStepSessionRegistry.snapshot()
        assertEquals(baseline.activeCount, restored.activeCount)
        assertEquals(baseline.expectedPackageNames, restored.expectedPackageNames)
    }
}
