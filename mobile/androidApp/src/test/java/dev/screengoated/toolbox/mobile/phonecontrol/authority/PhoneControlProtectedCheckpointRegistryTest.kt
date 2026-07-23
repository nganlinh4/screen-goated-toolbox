package dev.screengoated.toolbox.mobile.phonecontrol.authority

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PhoneControlProtectedCheckpointRegistryTest {
    @Test
    fun `only the owning token can reopen model tools`() {
        val before = PhoneControlProtectedCheckpointRegistry.snapshot()
        assertFalse(before.active)

        val owner = PhoneControlProtectedCheckpointRegistry.begin()
        val active = PhoneControlProtectedCheckpointRegistry.snapshot()
        assertTrue(active.active)
        assertTrue(active.generation > before.generation)
        assertTrue(PhoneControlProtectedCheckpointRegistry.owns(owner))
        assertFalse(PhoneControlProtectedCheckpointRegistry.modelToolsAllowed())

        val stale = PhoneControlProtectedCheckpointToken(owner.id + 1)
        assertFalse(PhoneControlProtectedCheckpointRegistry.end(stale))
        assertFalse(PhoneControlProtectedCheckpointRegistry.modelToolsAllowed())

        assertTrue(PhoneControlProtectedCheckpointRegistry.end(owner))
        assertTrue(PhoneControlProtectedCheckpointRegistry.modelToolsAllowed())
        assertTrue(
            PhoneControlProtectedCheckpointRegistry.snapshot().generation > active.generation,
        )
    }

    @Test(expected = IllegalStateException::class)
    fun `overlapping checkpoints are rejected`() {
        val owner = PhoneControlProtectedCheckpointRegistry.begin()
        try {
            PhoneControlProtectedCheckpointRegistry.begin()
        } finally {
            PhoneControlProtectedCheckpointRegistry.end(owner)
        }
    }
}
