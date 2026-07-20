package dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility

import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class AccessibilityGestureOutcomeTest {
    @Test
    fun rejectedBeforeDispatchIsTheOnlyProvenNoEffectGestureOutcome() {
        val outcome = gestureDispatchOutcome(
            GestureDispatchCompletion.REJECTED_BEFORE_DISPATCH,
            GENERATION,
        )

        assertEquals("gesture_rejected", outcome.code)
        assertEquals(EffectCertainty.PROVEN_NO_EFFECT, outcome.effect)
        assertFalse(outcome.snapshotInvalidated)
    }

    @Test
    fun acceptedThenCancelledRequiresReconciliation() {
        val outcome = gestureDispatchOutcome(
            GestureDispatchCompletion.CANCELLED_AFTER_ACCEPTANCE,
            GENERATION,
        )

        assertEquals("gesture_cancelled", outcome.code)
        assertEquals(EffectCertainty.MAY_HAVE_OCCURRED, outcome.effect)
        assertTrue(outcome.snapshotInvalidated)
    }

    @Test
    fun completedGestureRetainsTheConservativeEffectContract() {
        val outcome = gestureDispatchOutcome(GestureDispatchCompletion.COMPLETED, GENERATION)

        assertEquals("ok", outcome.code)
        assertEquals(EffectCertainty.MAY_HAVE_OCCURRED, outcome.effect)
        assertTrue(outcome.snapshotInvalidated)
    }
}

private const val GENERATION = 17L
