package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import dev.screengoated.toolbox.mobile.phonecontrol.GeneratedPhoneControlContract
import org.junit.Assert.assertEquals
import org.junit.Test

class PhoneControlRuntimeStatusPublisherTest {
    @Test
    fun `unchanged runtime and orb snapshots do not wake the overlay`() {
        val snapshots = mutableListOf<PhoneControlRuntimeSnapshot>()
        val publisher = PhoneControlRuntimeStatusPublisher(
            observer = PhoneControlRuntimeObserver(snapshots::add),
            isTransportReady = { true },
        )

        publisher.publish(
            phase = PhoneControlRuntimePhase.WORKING,
            code = PhoneControlRuntimeCode.WORKING,
            message = "Working",
        )
        publisher.publish(
            phase = PhoneControlRuntimePhase.WORKING,
            code = PhoneControlRuntimeCode.WORKING,
            message = "Working",
        )
        publisher.updateOrbPresentation(
            GeneratedPhoneControlContract.ORB_STATE_RESPONDING,
            null,
        )
        publisher.updateOrbPresentation(
            GeneratedPhoneControlContract.ORB_STATE_RESPONDING,
            null,
            preserveCurrentIconOnNull = true,
        )

        assertEquals(2, snapshots.size)
    }

    @Test
    fun `responding icon survives repeated transcript presentation and changes once`() {
        val snapshots = mutableListOf<PhoneControlRuntimeSnapshot>()
        val publisher = PhoneControlRuntimeStatusPublisher(
            observer = PhoneControlRuntimeObserver(snapshots::add),
            isTransportReady = { true },
        )

        publisher.updateOrbPresentation(
            GeneratedPhoneControlContract.ORB_STATE_RESPONDING,
            "sentiment_excited",
        )
        publisher.updateOrbPresentation(
            GeneratedPhoneControlContract.ORB_STATE_RESPONDING,
            null,
            preserveCurrentIconOnNull = true,
        )

        assertEquals(1, snapshots.size)
        assertEquals("sentiment_excited", snapshots.single().orbIconOverride)
    }
}
