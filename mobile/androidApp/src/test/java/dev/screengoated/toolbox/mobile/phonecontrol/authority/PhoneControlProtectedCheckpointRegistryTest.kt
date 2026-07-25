package dev.screengoated.toolbox.mobile.phonecontrol.authority

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PhoneControlProtectedCheckpointRegistryTest {
    @Test
    fun `only the owning token can reopen model tools`() {
        val before = PhoneControlProtectedCheckpointRegistry.snapshot()
        assertFalse(before.active)

        val owner = PhoneControlProtectedCheckpointRegistry.begin(
            PhoneControlProtectedCapturePolicy.RELEASE_PROJECTION,
        )
        val active = PhoneControlProtectedCheckpointRegistry.snapshot()
        assertTrue(active.active)
        assertTrue(active.freshProjectionRequired)
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
        val owner = PhoneControlProtectedCheckpointRegistry.begin(
            PhoneControlProtectedCapturePolicy.RETAIN_PROJECTION,
        )
        try {
            PhoneControlProtectedCheckpointRegistry.begin(
                PhoneControlProtectedCapturePolicy.RELEASE_PROJECTION,
            )
        } finally {
            PhoneControlProtectedCheckpointRegistry.end(owner)
        }
    }

    @Test
    fun `only release policy requires a fresh projection`() {
        val retained = PhoneControlProtectedCheckpointRegistry.begin(
            PhoneControlProtectedCapturePolicy.RETAIN_PROJECTION,
        )
        assertFalse(PhoneControlProtectedCheckpointRegistry.freshProjectionRequired())
        assertFalse(PhoneControlProtectedCheckpointRegistry.snapshot().freshProjectionRequired)
        assertTrue(PhoneControlProtectedCheckpointRegistry.end(retained))

        val released = PhoneControlProtectedCheckpointRegistry.begin(
            PhoneControlProtectedCapturePolicy.RELEASE_PROJECTION,
        )
        assertTrue(PhoneControlProtectedCheckpointRegistry.freshProjectionRequired())
        assertTrue(PhoneControlProtectedCheckpointRegistry.end(released))
    }
}
