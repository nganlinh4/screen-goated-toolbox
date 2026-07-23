package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import dev.screengoated.toolbox.mobile.shared.live.GenerationPlaybackChunk
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

class PhoneControlPlaybackQueueTest {
    @Test
    fun `playback stays pending until the consumer finishes handing off the chunk`() = runBlocking {
        val queue = PhoneControlPlaybackQueue(capacity = 1)
        val playbackStarted = CountDownLatch(1)
        val allowPlaybackToFinish = CountDownLatch(1)
        val consumer = launch(Dispatchers.Default) {
            queue.consume {
                playbackStarted.countDown()
                assertTrue(allowPlaybackToFinish.await(TIMEOUT_SECONDS, TimeUnit.SECONDS))
            }
        }

        try {
            assertTrue(queue.offer(GenerationPlaybackChunk(epoch = 0, bytes = byteArrayOf(1))))
            assertTrue(playbackStarted.await(TIMEOUT_SECONDS, TimeUnit.SECONDS))
            assertFalse(queue.isDrained(pendingPlayerFrames = 0))
        } finally {
            queue.close()
            allowPlaybackToFinish.countDown()
            withTimeout(TIMEOUT_SECONDS * 1_000L) { consumer.join() }
        }
        assertTrue(queue.isDrained(pendingPlayerFrames = 0))
        assertFalse(queue.isDrained(pendingPlayerFrames = 1))
    }

    private companion object {
        const val TIMEOUT_SECONDS = 5L
    }
}
