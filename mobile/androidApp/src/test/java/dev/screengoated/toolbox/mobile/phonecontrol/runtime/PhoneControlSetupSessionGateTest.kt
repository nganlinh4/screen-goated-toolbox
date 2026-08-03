package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class PhoneControlSetupSessionGateTest {
    @Test
    fun `success admits input only after clean session and announcement`() {
        val gate = PhoneControlSetupSessionGate()

        assertTrue(gate.begin())
        assertFalse(gate.inputAdmitted)
        assertNull(gate.withAdmittedInput { "audio" })
        assertTrue(gate.finish(waitForAnnouncement = true))
        assertFalse(gate.observeFreshSession())
        assertFalse(gate.inputAdmitted)
        assertTrue(gate.observeAnnouncementFinished())
        assertTrue(gate.inputAdmitted)
    }

    @Test
    fun `cancel needs a clean session but no success announcement`() {
        val gate = PhoneControlSetupSessionGate()

        assertTrue(gate.begin())
        assertTrue(gate.finish(waitForAnnouncement = false))
        assertTrue(gate.observeFreshSession())
        assertTrue(gate.inputAdmitted)
    }
}
