package dev.screengoated.toolbox.mobile.service.nativelibs

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PlaySplitConfirmationLatchTest {
    @Test
    fun acceptedRequirementSuppressesDuplicatesButNotANewRequirement() {
        val latch = PlaySplitConfirmationLatch()

        assertTrue(latch.request(41))
        assertFalse(latch.request(41))
        latch.markAccepted(41)
        assertFalse(latch.request(41))

        latch.clearRequirement(41)
        assertTrue(latch.request(41))
    }
}
