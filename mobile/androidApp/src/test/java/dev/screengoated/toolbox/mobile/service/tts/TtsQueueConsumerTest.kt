package dev.screengoated.toolbox.mobile.service.tts

import java.util.concurrent.LinkedBlockingDeque
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

class TtsQueueConsumerTest {
    @Test
    fun idleConsumerParksWithoutATimeoutAndHandlesJobsInOrder() {
        val queue = LinkedBlockingDeque<Int>()
        val handled = LinkedBlockingQueue<Int>()
        val consumer = startTtsQueueConsumer("tts-consumer-test", queue, handled::put)
        try {
            // A timed poll would show TIMED_WAITING; an idle take() parks with no wakeup.
            awaitState(consumer, Thread.State.WAITING)
            queue.offer(1)
            queue.offer(2)
            assertEquals(1, handled.poll(5, TimeUnit.SECONDS))
            assertEquals(2, handled.poll(5, TimeUnit.SECONDS))
            awaitState(consumer, Thread.State.WAITING)
        } finally {
            consumer.interrupt()
            consumer.join(5_000)
        }
        assertFalse(consumer.isAlive)
    }

    @Test
    fun aHandlerThatRestoresTheInterruptStopsTheConsumer() {
        val queue = LinkedBlockingDeque<Int>()
        val handled = LinkedBlockingQueue<Int>()
        val consumer = startTtsQueueConsumer("tts-consumer-test", queue) { job ->
            handled.put(job)
            Thread.currentThread().interrupt()
        }
        queue.offer(1)
        queue.offer(2)
        consumer.join(5_000)
        assertFalse(consumer.isAlive)
        assertEquals(listOf(1), handled.toList())
        assertEquals(listOf(2), queue.toList())
    }

    private fun awaitState(thread: Thread, state: Thread.State) {
        val deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(5)
        while (thread.state != state) {
            check(System.nanoTime() < deadline) { "consumer stayed ${thread.state}" }
            Thread.onSpinWait()
        }
    }
}
