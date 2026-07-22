package dev.screengoated.toolbox.mobile.phonecontrol.overlay

import dev.screengoated.toolbox.mobile.service.DismissHit
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class PhoneControlOrbDragSessionTest {
    @Test
    fun movementBelowThresholdRemainsATap() {
        val session = PhoneControlOrbDragSession(thresholdPx = 5f)
        session.begin(rawX = 20f, rawY = 30f, windowX = 100, windowY = 200)

        assertNull(session.move(rawX = 24f, rawY = 34f))
        assertEquals(PhoneControlOrbDragRelease.TAP, session.release(hit = null))
    }

    @Test
    fun dragOutsideDismissTargetPersistsAsMovement() {
        val session = PhoneControlOrbDragSession(thresholdPx = 5f)
        session.begin(rawX = 20f, rawY = 30f, windowX = 100, windowY = 200)

        val update = requireNotNull(session.move(rawX = 32f, rawY = 50f))
        assertTrue(update.started)
        assertEquals(112, update.windowX)
        assertEquals(220, update.windowY)
        assertEquals(
            PhoneControlOrbDragRelease.MOVED,
            session.release(DismissHit(singleProximity = 0.79f, allProximity = 0f)),
        )
    }

    @Test
    fun dragInsideSharedSingleTargetCommitsDismiss() {
        val session = PhoneControlOrbDragSession(thresholdPx = 5f)
        session.begin(rawX = 20f, rawY = 30f, windowX = 100, windowY = 200)
        requireNotNull(session.move(rawX = 40f, rawY = 60f))

        assertEquals(
            PhoneControlOrbDragRelease.DISMISS,
            session.release(DismissHit(singleProximity = 0.8f, allProximity = 0f)),
        )
        assertFalse(session.dragging)
    }
}
