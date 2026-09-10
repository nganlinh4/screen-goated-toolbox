package dev.screengoated.toolbox.mobile.service.preset

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class StreamingPasteStateTest {
    @Test
    fun `provider failure retains paste suppression and rejects stale callbacks`() {
        val state = StreamingPasteState()
        val initial = state.generation
        assertTrue(state.accept(initial))
        state.recordInsertion(true)
        state.invalidate()
        assertTrue(state.suppressFinalPaste)
        assertTrue(state.inserted)
        assertFalse(state.accept(initial))
    }

    @Test
    fun `refused target cannot be bypassed by final batch paste`() {
        val state = StreamingPasteState()
        state.accept(state.generation)
        state.recordInsertion(false)
        state.invalidate()
        assertFalse(state.inserted)
        assertTrue(state.suppressFinalPaste)
    }

    @Test
    fun `new capture resets delivery without accepting old socket callbacks`() {
        val state = StreamingPasteState()
        val old = state.generation
        state.accept(old)
        state.reset()
        assertFalse(state.suppressFinalPaste)
        assertFalse(state.accept(old))
        assertTrue(state.accept(state.generation))
    }
}
