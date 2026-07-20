package dev.screengoated.toolbox.mobile.phonecontrol.effect

import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlEffectCertainty
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicReference
import kotlinx.coroutines.async
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import kotlinx.coroutines.yield
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class PhoneControlEffectOwnerTest {
    @Test
    fun cancellationBeforeOwnedDispatchIsProvenNoEffect() = runBlocking {
        val owner = owner("before-dispatch")
        val lease = requireNotNull(owner.beginEffect())

        assertEquals(PhoneControlEffectCertainty.PROVEN_NO_EFFECT, owner.requestCancellation())
        assertNull(lease.dispatchBooleanIfActive { true })
        lease.close()
        owner.awaitTerminalEffects()

        assertEquals(
            PhoneControlEffectCertainty.PROVEN_NO_EFFECT,
            owner.terminalCertainty(mutatingFallback = true),
        )
    }

    @Test
    fun cancellationBeforeBoundaryIsUnknownOnlyForUninstrumentedMutation() {
        val owner = owner("unowned-mutation")

        owner.requestCancellation()

        assertEquals(
            PhoneControlEffectCertainty.UNKNOWN,
            owner.terminalCertainty(mutatingFallback = true),
        )
        assertNull(owner.beginEffect())
        assertEquals(
            PhoneControlEffectCertainty.PROVEN_NO_EFFECT,
            owner.terminalCertainty(mutatingFallback = true),
        )
    }

    @Test
    fun acceptedEffectDoesNotAcknowledgeTerminalCancellationUntilLeaseCloses() = runBlocking {
        val owner = owner("accepted-effect")
        val lease = requireNotNull(owner.beginEffect())
        var dispatched = false
        assertTrue(lease.dispatchIfActive { dispatched = true })
        assertTrue(dispatched)

        assertEquals(PhoneControlEffectCertainty.MAY_HAVE_OCCURRED, owner.requestCancellation())
        val terminal = async { owner.awaitTerminalEffects() }
        yield()
        assertFalse(terminal.isCompleted)

        lease.close()
        terminal.await()
        assertEquals(
            PhoneControlEffectCertainty.MAY_HAVE_OCCURRED,
            owner.terminalCertainty(mutatingFallback = true),
        )
    }

    @Test
    fun platformDispatchExceptionCannotBeReportedAsNoEffect() {
        val owner = owner("dispatch-exception")
        val lease = requireNotNull(owner.beginEffect())

        runCatching {
            lease.dispatchBooleanIfActive { error("platform dispatch failed without a receipt") }
        }
        lease.close()
        owner.requestCancellation()

        assertEquals(
            PhoneControlEffectCertainty.MAY_HAVE_OCCURRED,
            owner.terminalCertainty(mutatingFallback = true),
        )
    }

    @Test
    fun blockingPlatformDispatchCannotBlockCancellationRequest() = runBlocking {
        val owner = owner("blocking-platform-dispatch")
        val lease = requireNotNull(owner.beginEffect())
        val dispatchEntered = CountDownLatch(1)
        val releaseDispatch = CountDownLatch(1)
        val dispatchFinished = CountDownLatch(1)
        val requestReturned = CountDownLatch(1)
        val requestCertainty = AtomicReference<PhoneControlEffectCertainty>()
        val dispatchThread = Thread {
            try {
                lease.dispatchBooleanIfActive {
                    dispatchEntered.countDown()
                    releaseDispatch.await()
                    false
                }
            } finally {
                lease.close()
                dispatchFinished.countDown()
            }
        }.apply { start() }

        assertTrue(dispatchEntered.await(TEST_TIMEOUT_MS, TimeUnit.MILLISECONDS))
        val cancellationThread = Thread {
            requestCertainty.set(owner.requestCancellation())
            requestReturned.countDown()
        }.apply { start() }
        try {
            assertTrue(
                "cancellation must not wait for a platform callback",
                requestReturned.await(PROMPT_TIMEOUT_MS, TimeUnit.MILLISECONDS),
            )
            assertEquals(
                PhoneControlEffectCertainty.MAY_HAVE_OCCURRED,
                requestCertainty.get(),
            )
            val terminal = async { owner.awaitTerminalEffects() }
            yield()
            assertFalse(terminal.isCompleted)

            releaseDispatch.countDown()
            assertTrue(dispatchFinished.await(TEST_TIMEOUT_MS, TimeUnit.MILLISECONDS))
            terminal.await()
            assertEquals(
                PhoneControlEffectCertainty.PROVEN_NO_EFFECT,
                owner.terminalCertainty(mutatingFallback = true),
            )
        } finally {
            releaseDispatch.countDown()
            dispatchThread.join(TEST_TIMEOUT_MS)
            cancellationThread.join(TEST_TIMEOUT_MS)
        }
    }

    @Test
    fun blockingCancellationHandlerIsOwnedWithoutBlockingRequestThread() = runBlocking {
        val owner = owner("blocking-remote-cancel")
        val handlerEntered = CountDownLatch(1)
        val releaseHandler = CountDownLatch(1)
        val requestReturned = CountDownLatch(1)
        requireNotNull(owner.registerCancellationHandler {
            handlerEntered.countDown()
            releaseHandler.await()
        })
        val requestThread = Thread {
            owner.requestCancellation()
            requestReturned.countDown()
        }.apply { start() }

        try {
            assertTrue(requestReturned.await(PROMPT_TIMEOUT_MS, TimeUnit.MILLISECONDS))
            assertTrue(handlerEntered.await(TEST_TIMEOUT_MS, TimeUnit.MILLISECONDS))
            val terminal = async { owner.awaitTerminalEffects() }
            yield()
            assertFalse(terminal.isCompleted)

            releaseHandler.countDown()
            withTimeout(TEST_TIMEOUT_MS) { terminal.await() }
        } finally {
            releaseHandler.countDown()
            requestThread.join(TEST_TIMEOUT_MS)
        }
    }

    @Test
    fun failedCancellationHandlerStillSettlesTerminalOwnership() = runBlocking {
        val owner = owner("failed-remote-cancel")
        requireNotNull(owner.registerCancellationHandler { error("dead remote") })

        owner.requestCancellation()

        withTimeout(TEST_TIMEOUT_MS) { owner.awaitTerminalEffects() }
        assertEquals(
            PhoneControlEffectCertainty.PROVEN_NO_EFFECT,
            owner.terminalCertainty(mutatingFallback = false),
        )
    }

    private fun owner(jobId: String) = PhoneControlEffectOwner(
        PhoneControlOperationId(turnId = 7, responseGeneration = 11, jobId = jobId),
    )

    private companion object {
        const val PROMPT_TIMEOUT_MS = 2_000L
        const val TEST_TIMEOUT_MS = 5_000L
    }
}
