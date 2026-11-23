package dev.screengoated.toolbox.mobile.phonecontrol

import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedCapturePolicy
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedCheckpointRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class PhoneControlProtectedSetupContinuationTest {
    @Test
    fun completedRelayResumesExactlyOnce() {
        val continuation = PhoneControlProtectedSetupContinuation()

        continuation.begin()
        continuation.relayCompleted()

        assertTrue(continuation.consumeResumeSelectedSetup())
        assertFalse(continuation.consumeResumeSelectedSetup())
    }

    @Test
    fun unresolvedRelayRestoresCaptureWithoutRepeatingSetup() {
        val continuation = PhoneControlProtectedSetupContinuation()

        continuation.begin()
        continuation.relayCompleted()
        continuation.relayNeedsUserStep()

        assertFalse(continuation.consumeResumeSelectedSetup())
    }

    @Test
    fun authorityChangeResumesOnlyAnotherElevatedProvider() {
        val continuation = PhoneControlProtectedSetupContinuation()

        continuation.authorityChanged(nextProviderNeedsSetup = true)
        assertTrue(continuation.consumeResumeSelectedSetup())

        continuation.authorityChanged(nextProviderNeedsSetup = false)
        assertFalse(continuation.consumeResumeSelectedSetup())
    }

    @Test
    fun acceptedFreshCaptureClearsFallbackForStandardAndElevatedChoices() {
        for (resume in listOf(false, true)) {
            val continuation = PhoneControlProtectedSetupContinuation()
            continuation.authorityChanged(nextProviderNeedsSetup = resume)
            var notificationsCleared = 0

            assertTrue(continuation.projectionAttached { notificationsCleared += 1 } == resume)
            assertTrue(notificationsCleared == 1)
            assertFalse(continuation.consumeResumeSelectedSetup())
        }
    }

    @Test
    fun staleOrRejectedCaptureKeepsFallbackAndReleasesCapture() {
        val stale = PhoneControlProtectedCheckpointRegistry.begin(
            PhoneControlProtectedCapturePolicy.RELEASE_PROJECTION,
        )
        PhoneControlProtectedCheckpointRegistry.end(stale)
        val owner = PhoneControlProtectedCheckpointRegistry.begin(
            PhoneControlProtectedCapturePolicy.RELEASE_PROJECTION,
        )
        try {
            var releases = 0
            var accepted = 0
            var attachCalls = 0
            assertFalse(acceptFreshProtectedProjection(
                stale,
                attach = { attachCalls += 1; true },
                release = { releases += 1 },
                accepted = { accepted += 1 },
            ))
            assertEquals(0, attachCalls)
            assertFalse(acceptFreshProtectedProjection(
                owner,
                attach = { false },
                release = { releases += 1 },
                accepted = { accepted += 1 },
            ))
            assertEquals(2, releases)
            assertEquals(0, accepted)
            assertTrue(PhoneControlProtectedCheckpointRegistry.owns(owner))
            assertTrue(acceptFreshProtectedProjection(
                owner,
                attach = { PhoneControlProtectedCheckpointRegistry.end(it) },
                release = { releases += 1 },
                accepted = { accepted += 1 },
            ))
            assertEquals(2, releases)
            assertEquals(1, accepted)
        } finally {
            PhoneControlProtectedCheckpointRegistry.end(owner)
        }
    }

    @Test
    fun delayedConsentCannotStartProjectionForANewerCheckpoint() {
        val first = PhoneControlProtectedCheckpointRegistry.begin(
            PhoneControlProtectedCapturePolicy.RELEASE_PROJECTION,
        )
        val firstOwner = PhoneControlProtectedCheckpointRegistry.freshProjectionOwner()
        assertTrue(PhoneControlProtectedCheckpointRegistry.end(first))
        val second = PhoneControlProtectedCheckpointRegistry.begin(
            PhoneControlProtectedCapturePolicy.RELEASE_PROJECTION,
        )
        val generation = PhoneControlProjectionDispatches.reserveConsent()
        try {
            val secondOwner = PhoneControlProtectedCheckpointRegistry.freshProjectionOwner()
            var starts = 0
            assertFalse(canDeliverFreshProjectionConsent(firstOwner, generation, resumeMode = true))
            assertFalse(canDeliverFreshProjectionConsent(secondOwner, generation, resumeMode = false))
            assertTrue(canDeliverFreshProjectionConsent(secondOwner, generation, resumeMode = true))
            assertNull(startOwnedFreshProtectedProjection(second, firstOwner) { starts += 1 })
            assertNull(startOwnedFreshProtectedProjection(second, null) { starts += 1 })
            assertEquals(0, starts)
            assertTrue(PhoneControlProtectedCheckpointRegistry.owns(second))
            assertEquals(1, startOwnedFreshProtectedProjection(second, secondOwner) { ++starts })
            assertEquals(1, starts)
        } finally {
            PhoneControlProjectionDispatches.releaseConsent(generation)
            PhoneControlProtectedCheckpointRegistry.end(second)
        }
    }
}
