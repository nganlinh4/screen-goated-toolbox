package dev.screengoated.toolbox.mobile.service.preset

import org.junit.Assert.*
import org.junit.Test

class PendingStreamingAudioTest {
    @Test fun overflowFailsWithoutDroppingOrChangingQueuedAudio() {
        val pending = PendingStreamingAudio(limit = 4)
        val first = shortArrayOf(1, 2, 3)
        pending.append(first)
        first[0] = 9
        assertThrows(IllegalStateException::class.java) { pending.append(shortArrayOf(4, 5)) }
        assertArrayEquals(shortArrayOf(1, 2, 3), pending.removeFirstOrNull())
        assertNull(pending.removeFirstOrNull())
        pending.append(shortArrayOf(6, 7, 8, 9))
        pending.clear()
        assertNull(pending.removeFirstOrNull())
        pending.append(shortArrayOf(10))
        assertArrayEquals(shortArrayOf(10), pending.removeFirstOrNull())
    }
}
