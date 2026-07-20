package dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility

import dev.screengoated.toolbox.mobile.phonecontrol.effect.PhoneControlEffectOwner
import dev.screengoated.toolbox.mobile.phonecontrol.effect.PhoneControlOperationId
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlEffectCertainty
import kotlinx.coroutines.async
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import kotlinx.coroutines.yield
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class AccessibilityEffectOwnershipTest {
    @Test
    fun acceptedGestureRemainsOwnedAfterCancellationUntilPlatformCallback() = runBlocking {
        val owner = owner("gesture-callback")
        val effect = withContext(owner) { OwnedAccessibilityEffect.begin() }
        var platformAccepted = false
        assertTrue(effect.dispatch { platformAccepted = true })
        assertTrue(platformAccepted)

        assertEquals(PhoneControlEffectCertainty.MAY_HAVE_OCCURRED, owner.requestCancellation())
        val terminal = async { owner.awaitTerminalEffects() }
        yield()
        assertFalse(terminal.isCompleted)

        effect.close() // Simulated Accessibility GestureResultCallback terminal signal.
        terminal.await()
    }

    @Test
    fun cancellationWinningBeforeAccessibilityDispatchNeverCallsPlatform() = runBlocking {
        val owner = owner("gesture-before-dispatch")
        val effect = withContext(owner) { OwnedAccessibilityEffect.begin() }
        owner.requestCancellation()
        var dispatched = false

        assertFalse(effect.dispatch { dispatched = true })
        assertFalse(dispatched)
        effect.close()
        owner.awaitTerminalEffects()
        assertEquals(
            PhoneControlEffectCertainty.PROVEN_NO_EFFECT,
            owner.terminalCertainty(mutatingFallback = true),
        )
    }

    private fun owner(jobId: String) = PhoneControlEffectOwner(
        PhoneControlOperationId(turnId = 3, responseGeneration = 5, jobId = jobId),
    )
}
