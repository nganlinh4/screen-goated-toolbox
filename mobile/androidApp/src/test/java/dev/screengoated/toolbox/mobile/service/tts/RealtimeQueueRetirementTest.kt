package dev.screengoated.toolbox.mobile.service.tts

import java.util.concurrent.ConcurrentLinkedQueue
import java.util.concurrent.CountDownLatch
import java.util.concurrent.LinkedBlockingDeque
import java.util.concurrent.TimeUnit
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class RealtimeQueueRetirementTest {
    private data class Job(val id: Long, val consumer: TtsConsumer, val owner: String)

    @Test
    fun `queued retirement preserves other requests and releases capacity`() {
        val bus = TtsPlaybackEventBus(maxOutstanding = 3, maxPerOwner = 1, maxPerConsumer = 3)
        val read = Job(1, TtsConsumer.REALTIME, "read")
        val other = Job(2, TtsConsumer.REALTIME, "other")
        val preview = Job(3, TtsConsumer.SETTINGS_PREVIEW, "read")
        val queue = LinkedBlockingDeque(listOf(read, other, preview))
        for (job in queue) assertTrue(bus.reserve(job.id, job.consumer, job.owner))
        val matches = { job: Job -> matchesRealtimeOwner(1, "read", job.id, job.consumer, job.owner) }
        assertTrue(retireMatchingQueuedPlayback(queue, matches) { job ->
            bus.publish(TtsPlaybackEvent(job.id, job.consumer, job.owner, TtsCompletionStatus.INTERRUPTED))
        })
        assertFalse(retireMatchingQueuedPlayback(queue, matches) { error("duplicate retirement") })
        assertEquals(listOf(other, preview), queue.toList())
        assertTrue(bus.reserve(4, TtsConsumer.REALTIME, "read"))
        assertFalse(matchesRealtimeOwner(1, "read", 1, TtsConsumer.SETTINGS_PREVIEW, "read"))
        assertFalse(matchesRealtimeOwner(1, "read", 1, TtsConsumer.REALTIME, "other"))
    }

    @Test
    fun `dequeue racing scoped retirement gives terminal responsibility to one owner`() {
        repeat(500) {
            val queue = LinkedBlockingDeque(listOf(Job(1, TtsConsumer.REALTIME, "read")))
            val terminals = ConcurrentLinkedQueue<Long>()
            val start = CountDownLatch(1)
            val player = Thread {
                start.await()
                queue.poll()?.let { terminals.add(it.id) }
            }
            player.start()
            start.countDown()
            retireMatchingQueuedPlayback(queue, { it.id == 1L }) { terminals.add(it.id) }
            player.join(TimeUnit.SECONDS.toMillis(5))
            assertFalse(player.isAlive)
            assertEquals(listOf(1L), terminals.toList())
        }
    }
}
