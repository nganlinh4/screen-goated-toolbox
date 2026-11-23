package dev.screengoated.toolbox.mobile.capture

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class CaptureGenerationFenceTest {
    @Test
    fun restartPrecedesNewReadsAndCrossingReadsAreDiscarded() {
        var generation = 0L
        var restarts = 0
        val fence = CaptureGenerationFence({ generation }) { restarts++ }
        val oldRead = fence.prepareRead()
        generation++
        assertFalse(fence.acceptsRead(oldRead))
        val freshRead = fence.prepareRead()
        assertEquals(1, restarts)
        assertTrue(fence.acceptsRead(freshRead))
        fence.prepareRead()
        assertEquals(1, restarts)
    }

    @Test
    fun failedRestartDoesNotAdmitNewGeneration() {
        var generation = 0L
        var fail = true
        var attempts = 0
        val fence = CaptureGenerationFence({ generation }) {
            attempts++
            check(!fail)
        }
        generation++
        assertTrue(runCatching { fence.prepareRead() }.isFailure)
        fail = false
        assertEquals(1L, fence.prepareRead())
        assertEquals(2, attempts)
    }
}
