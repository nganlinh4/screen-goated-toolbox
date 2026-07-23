package dev.screengoated.toolbox.mobile.phonecontrol

import org.junit.Assert.assertFalse
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
}
