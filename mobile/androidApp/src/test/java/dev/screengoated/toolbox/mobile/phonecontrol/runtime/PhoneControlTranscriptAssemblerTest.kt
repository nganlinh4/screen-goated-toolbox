package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PhoneControlTranscriptAssemblerTest {
    @Test
    fun `cumulative overlap and duplicate fragments stay in one epoch`() {
        val assembler = PhoneControlInputTranscriptAssembler()

        val first = requireNotNull(assembler.merge("open settings"))
        val cumulative = requireNotNull(assembler.merge("open settings now"))
        val overlap = requireNotNull(assembler.merge("now please"))
        val duplicate = requireNotNull(assembler.merge("now please"))

        assertTrue(first.startsTurn)
        assertFalse(cumulative.startsTurn)
        assertFalse(overlap.startsTurn)
        assertEquals("open settings now please", overlap.text)
        assertFalse(duplicate.changed)
        assertEquals("open settings now please", assembler.text)
    }

    @Test
    fun `only an explicit epoch signal starts another turn`() {
        val assembler = PhoneControlInputTranscriptAssembler()
        assembler.merge("first request")

        val lateRevision = requireNotNull(assembler.merge("first request revised"))
        assembler.beginEpoch()
        val next = requireNotNull(assembler.merge("second request"))

        assertFalse(lateRevision.startsTurn)
        assertTrue(next.startsTurn)
        assertEquals("second request", next.text)
    }

    @Test
    fun `overlap boundaries never split a unicode code point`() {
        val merged = mergePhoneControlTranscriptText(
            existing = "show 😀ab",
            incoming = "😀ab now",
        )

        assertEquals("show 😀ab now", merged)
        assertFalse(merged.contains('�'))
    }

    @Test
    fun `claiming an empty fresh epoch binds late text to the active turn`() {
        val assembler = PhoneControlInputTranscriptAssembler()
        assembler.beginEpoch()
        assembler.claimCurrentEpoch()

        val late = requireNotNull(assembler.merge("correlated transcript"))

        assertFalse(late.startsTurn)
        assertEquals("correlated transcript", late.text)
    }
}
