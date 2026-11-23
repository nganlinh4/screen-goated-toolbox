package dev.screengoated.toolbox.mobile.service.tts

import java.util.concurrent.ConcurrentLinkedQueue
import java.util.concurrent.CountDownLatch
import java.util.concurrent.LinkedBlockingDeque
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class TtsPlaybackEventBusTest {
    @Test
    fun `slow subscriber receives all terminal events in order`() = runBlocking {
            val bus = TtsPlaybackEventBus(maxOutstanding = 256, maxPerOwner = 256, maxPerConsumer = 256, maxPerSubscriber = 256)
            val release = CompletableDeferred<Unit>()
            val first = CompletableDeferred<Unit>()
            val done = CompletableDeferred<Unit>()
            val received = mutableListOf<Long>()
            val subscriber = launch(start = CoroutineStart.UNDISPATCHED) {
                bus.eventsFor(TtsEventSelector(TtsConsumer.REALTIME, "owner")).collect { event ->
                    received.add(event.requestId)
                    if (event.requestId == 1L) { first.complete(Unit); release.await() }
                    if (received.size == 256) done.complete(Unit)
                }
            }
            for (id in 1L..256L) {
                assertTrue(bus.reserve(id, TtsConsumer.REALTIME, "owner"))
                bus.publish(TtsPlaybackEvent(id, TtsConsumer.REALTIME, "owner", TtsCompletionStatus.INTERRUPTED))
            }
            withTimeout(5_000) { first.await() }
            release.complete(Unit)
            withTimeout(5_000) { done.await() }
            assertEquals((1L..256L).toList(), received)
            subscriber.cancelAndJoin()
    }

    @Test
    fun `stalled owner is bounded and cannot block another owner`() = runBlocking {
        val bus = TtsPlaybackEventBus(maxOutstanding = 4, maxPerOwner = 2, maxPerConsumer = 4)
        val entered = CompletableDeferred<Unit>()
        val release = CompletableDeferred<Unit>()
        val otherDelivered = CompletableDeferred<Unit>()
        val stalled = launch(start = CoroutineStart.UNDISPATCHED) {
            bus.eventsFor(TtsEventSelector(TtsConsumer.REALTIME, "stalled")).collect {
                entered.complete(Unit)
                release.await()
            }
        }
        val other = launch(start = CoroutineStart.UNDISPATCHED) {
            bus.eventsFor(TtsEventSelector(TtsConsumer.REALTIME, "other")).collect { otherDelivered.complete(Unit) }
        }
        assertTrue(bus.reserve(1, TtsConsumer.REALTIME, "stalled"))
        bus.publish(TtsPlaybackEvent(1, TtsConsumer.REALTIME, "stalled", TtsCompletionStatus.INTERRUPTED))
        withTimeout(5_000) { entered.await() }
        assertTrue(bus.reserve(2, TtsConsumer.REALTIME, "stalled"))
        bus.publish(TtsPlaybackEvent(2, TtsConsumer.REALTIME, "stalled", TtsCompletionStatus.INTERRUPTED))
        assertFalse(bus.reserve(3, TtsConsumer.REALTIME, "stalled"))
        assertTrue(bus.reserve(4, TtsConsumer.REALTIME, "other"))
        bus.publish(TtsPlaybackEvent(4, TtsConsumer.REALTIME, "other", TtsCompletionStatus.COMPLETED))
        withTimeout(5_000) { otherDelivered.await() }
        stalled.cancelAndJoin()
        assertTrue(bus.reserve(3, TtsConsumer.REALTIME, "stalled"))
        bus.abandon(3)
        other.cancelAndJoin()
    }

    @Test
    fun `player dequeue and interruption partition pending jobs exactly once`() {
        repeat(100) {
            val queue = LinkedBlockingDeque((0 until 100).toList())
            val played = ConcurrentLinkedQueue<Int>()
            val interrupted = ConcurrentLinkedQueue<Int>()
            val ready = CountDownLatch(1)
            val player = Thread {
                ready.countDown()
                while (true) played.add(queue.poll() ?: break)
            }
            player.start()
            assertTrue(ready.await(5, TimeUnit.SECONDS))
            retireQueuedPlayback(queue) { interrupted.add(it) }
            player.join(5_000)
            assertTrue(!player.isAlive)
            assertEquals((0 until 100).toList(), (played + interrupted).sorted())
        }
    }

    @Test
    fun `union subscriber sees realtime and preview terminals in publication order`() = runBlocking {
        val bus = TtsPlaybackEventBus(maxOutstanding = 4, maxPerOwner = 4, maxPerConsumer = 4)
        val events = mutableListOf<TtsPlaybackEvent>()
        val done = CompletableDeferred<Unit>()
        val subscription = launch(start = CoroutineStart.UNDISPATCHED) {
            bus.eventsFor(
                TtsEventSelector(TtsConsumer.REALTIME, "live"),
                TtsEventSelector(TtsConsumer.SETTINGS_PREVIEW),
            ).collect {
                events.add(it)
                if (events.size == 2) done.complete(Unit)
            }
        }
        assertTrue(bus.reserve(1, TtsConsumer.REALTIME, "live"))
        assertTrue(bus.reserve(2, TtsConsumer.SETTINGS_PREVIEW, "preview"))
        bus.publish(TtsPlaybackEvent(1, TtsConsumer.REALTIME, "live", TtsCompletionStatus.INTERRUPTED))
        bus.publish(TtsPlaybackEvent(2, TtsConsumer.SETTINGS_PREVIEW, "preview", TtsCompletionStatus.COMPLETED))
        withTimeout(5_000) { done.await() }
        assertEquals(listOf(1L, 2L), events.map { it.requestId })
        subscription.cancelAndJoin()
    }

    @Test
    fun `stalled subscriber cannot exhaust capacity with distinct owners`() = runBlocking {
        val bus = TtsPlaybackEventBus()
        val entered = CompletableDeferred<Unit>()
        val release = CompletableDeferred<Unit>()
        val delivered = CompletableDeferred<Unit>()
        val stalled = launch(start = CoroutineStart.UNDISPATCHED) {
            bus.eventsFor(TtsEventSelector(TtsConsumer.AUTO_SPEAK, "stalled-")).collect {
                entered.complete(Unit)
                release.await()
            }
        }
        val other = launch(start = CoroutineStart.UNDISPATCHED) {
            bus.eventsFor(TtsEventSelector(TtsConsumer.REALTIME, "other")).collect { delivered.complete(Unit) }
        }
        var accepted = 0
        for (id in 1L..600L) {
            val owner = "stalled-$id"
            if (bus.reserve(id, TtsConsumer.AUTO_SPEAK, owner)) {
                accepted++
                bus.publish(TtsPlaybackEvent(id, TtsConsumer.AUTO_SPEAK, owner, TtsCompletionStatus.INTERRUPTED))
            }
        }
        withTimeout(5_000) { entered.await() }
        assertEquals(32, accepted)
        assertTrue(bus.reserve(601, TtsConsumer.REALTIME, "other"))
        bus.publish(TtsPlaybackEvent(601, TtsConsumer.REALTIME, "other", TtsCompletionStatus.COMPLETED))
        withTimeout(5_000) { delivered.await() }
        stalled.cancelAndJoin()
        assertTrue(bus.reserve(602, TtsConsumer.AUTO_SPEAK, "stalled-new"))
        bus.abandon(602)
        other.cancelAndJoin()
    }
}
