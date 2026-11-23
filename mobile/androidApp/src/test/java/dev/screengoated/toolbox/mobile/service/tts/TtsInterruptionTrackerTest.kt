package dev.screengoated.toolbox.mobile.service.tts

import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import kotlin.concurrent.thread
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class TtsInterruptionTrackerTest {
    @Test
    fun firstPreviewCauseSurvivesSubsequentPreviewAndTerminalOrder() {
        val tracker = TtsInterruptionTracker()
        // Register before the player claims the request, so the player-to-active gap is covered.
        assertTrue(tracker.register(1))
        assertTrue(tracker.register(2))
        tracker.interrupt(2)
        assertTrue(tracker.register(3))
        tracker.interrupt(3)

        // Preview 2 may be retired before the older player publishes request 1.
        val secondPreview = tracker.terminal(2)
        assertEquals(3L, secondPreview?.replacementRequestId)
        val firstRequest = tracker.terminal(1)
        assertEquals(2L, firstRequest?.replacementRequestId)
        assertTrue(tracker.resumeAllowed(checkNotNull(firstRequest)))
        assertNull(tracker.terminal(1))
        assertTrue(tracker.resumeAllowed(checkNotNull(tracker.terminal(3))))
    }

    @Test
    fun explicitStopInvalidatesAlreadyPublishedAndStillPendingPreviewCauses() {
        val tracker = TtsInterruptionTracker()
        assertTrue(tracker.register(1))
        assertTrue(tracker.register(2))
        assertTrue(tracker.register(3))
        tracker.interrupt(3)
        val publishedBeforeStop = checkNotNull(tracker.terminal(1))
        assertTrue(tracker.resumeAllowed(publishedBeforeStop))

        tracker.interrupt(null)
        assertFalse(tracker.resumeAllowed(publishedBeforeStop))
        val publishedAfterStop = tracker.terminal(2)
        if (publishedAfterStop != null) assertFalse(tracker.resumeAllowed(publishedAfterStop))
        assertFalse(tracker.resumeAllowed(checkNotNull(tracker.terminal(3))))

        // A later preview belongs to the new stop epoch and may resume normally.
        assertTrue(tracker.register(4))
        assertTrue(tracker.register(5))
        tracker.interrupt(5)
        assertTrue(tracker.resumeAllowed(checkNotNull(tracker.terminal(4))))
    }

    @Test
    fun concurrentTerminalClaimsProduceOnlyOneInterruption() {
        val tracker = TtsInterruptionTracker()
        assertTrue(tracker.register(1))
        tracker.interrupt(2)
        val ready = CountDownLatch(2)
        val start = CountDownLatch(1)
        val claimed = AtomicInteger()
        val workers = List(2) {
            thread(isDaemon = true) {
                ready.countDown()
                if (start.await(2, TimeUnit.SECONDS) && tracker.terminal(1) != null) claimed.incrementAndGet()
            }
        }
        try {
            assertTrue(ready.await(2, TimeUnit.SECONDS))
            start.countDown()
            workers.forEach { it.join(2_000) }
            assertTrue(workers.none { it.isAlive })
            assertEquals(1, claimed.get())
        } finally {
            start.countDown()
        }
    }

    @Test
    fun retirementReleasesBoundedRegistrationCapacity() {
        val tracker = TtsInterruptionTracker(maxOutstanding = 1)
        assertFalse(tracker.register(0))
        assertTrue(tracker.register(1))
        assertFalse(tracker.register(1))
        assertFalse(tracker.register(2))
        tracker.interrupt(2)
        assertNotNull(tracker.terminal(1))
        assertTrue(tracker.register(2))
    }
}
