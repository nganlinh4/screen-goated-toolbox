package dev.screengoated.toolbox.mobile.phonecontrol.tools

import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityGestureOutcome
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityProviderResult
import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ElevatedPointerInputTest {
    @Test
    fun `proven accessibility rejection may use the selected elevated effect provider`() = runTest {
        var elevatedCalled = false
        val result = routePointerInput(failure("gesture_rejected"), { 7L }) {
            elevatedCalled = true
            elevatedSuccess()
        }

        assertTrue(elevatedCalled)
        assertEquals("sgt_adb_bridge", result.providerId)
        assertEquals(EffectCertainty.MAY_HAVE_OCCURRED, result.effect)
    }

    @Test
    fun `accepted or uncertain accessibility input never dispatches a second effect`() = runTest {
        var elevatedCalled = false
        val accepted = routePointerInput(
            AccessibilityProviderResult.Success(
                AccessibilityGestureOutcome(
                    code = "ok",
                    generation = 7L,
                    effect = EffectCertainty.MAY_HAVE_OCCURRED,
                    snapshotInvalidated = true,
                ),
            ),
            { 7L },
        ) {
            elevatedCalled = true
            elevatedSuccess()
        }
        val uncertain = routePointerInput(
            failure("gesture_rejected", EffectCertainty.MAY_HAVE_OCCURRED),
            { 7L },
        ) {
            elevatedCalled = true
            elevatedSuccess()
        }

        assertFalse(elevatedCalled)
        assertEquals("accessibility", accepted.providerId)
        assertEquals("accessibility", uncertain.providerId)
    }

    @Test
    fun `unavailable elevated route preserves the original typed rejection`() = runTest {
        var elevatedCalled = false
        val result = routePointerInput(failure("action_rejected"), { 7L }) {
            elevatedCalled = true
            null
        }

        assertTrue(elevatedCalled)
        assertEquals("accessibility", result.providerId)
        assertEquals("action_rejected", result.code)
        assertEquals(EffectCertainty.PROVEN_NO_EFFECT, result.effect)
    }

    private fun failure(
        code: String,
        effect: EffectCertainty = EffectCertainty.PROVEN_NO_EFFECT,
    ) = AccessibilityProviderResult.Failure(
        code = code,
        message = "typed provider failure",
        retryable = true,
        effect = effect,
    )

    private fun elevatedSuccess() = PointerInputOutcome(
        providerId = "sgt_adb_bridge",
        providerState = CapabilityState.READY,
        code = "ok",
        generation = 8L,
        effect = EffectCertainty.MAY_HAVE_OCCURRED,
        snapshotInvalidated = true,
    )
}
