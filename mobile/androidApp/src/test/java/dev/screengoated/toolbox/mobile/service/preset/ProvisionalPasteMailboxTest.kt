package dev.screengoated.toolbox.mobile.service.preset

import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class ProvisionalPasteMailboxTest {
    @Test
    fun `adjacent interims coalesce without crossing a final boundary`() {
        val mailbox = ProvisionalPasteMailbox()
        mailbox.enqueue(ProvisionalPasteEvent.Interim("old"))
        mailbox.enqueue(ProvisionalPasteEvent.Interim("new"))
        mailbox.enqueue(ProvisionalPasteEvent.Final("first"))
        mailbox.enqueue(ProvisionalPasteEvent.Interim("next"))
        mailbox.enqueue(ProvisionalPasteEvent.Final("second"))
        assertEquals(ProvisionalPasteEvent.Interim("new"), mailbox.poll())
        assertEquals(ProvisionalPasteEvent.Final("first"), mailbox.poll())
        assertEquals(ProvisionalPasteEvent.Interim("next"), mailbox.poll())
        assertEquals(ProvisionalPasteEvent.Final("second"), mailbox.poll())
        assertNull(mailbox.poll())
    }

    @Test
    fun `stop discards hypotheses retains ordered finals and permanently closes enqueue`() {
        val mailbox = ProvisionalPasteMailbox()
        mailbox.enqueue(ProvisionalPasteEvent.Interim("draft"))
        mailbox.enqueue(ProvisionalPasteEvent.Final("one"))
        mailbox.beginDrain()
        assertFalse(mailbox.enqueue(ProvisionalPasteEvent.Interim("late")))
        assertTrue(mailbox.enqueue(ProvisionalPasteEvent.Final("two")))
        mailbox.close(discard = false)
        assertFalse(mailbox.enqueue(ProvisionalPasteEvent.Final("stale")))
        assertEquals(ProvisionalPasteEvent.Final("one"), mailbox.poll())
        assertEquals(ProvisionalPasteEvent.Final("two"), mailbox.poll())
        assertEquals(ProvisionalPasteEvent.Finish, mailbox.poll())
        assertNull(mailbox.poll())
    }

    @Test
    fun `overflow is bounded by event count and UTF8 bytes and leaves cleanup only`() {
        val countBound = ProvisionalPasteMailbox()
        repeat(129) { countBound.enqueue(ProvisionalPasteEvent.Final("x")) }
        assertEquals(ProvisionalPasteEvent.Finish, countBound.poll())
        assertNull(countBound.poll())
        assertFalse(countBound.enqueue(ProvisionalPasteEvent.Final("late")))
        val byteBound = ProvisionalPasteMailbox()
        byteBound.enqueue(ProvisionalPasteEvent.Interim("🙂".repeat(32_769)))
        assertEquals(ProvisionalPasteEvent.Finish, byteBound.poll())
        assertNull(byteBound.poll())
    }

    @Test
    fun `single consumer revises newest hypothesis and commits finals in order`() = runTest {
        val target = Target()
        val delivery = ProvisionalPasteDelivery(this, ProvisionalPasteSession(target))
        delivery.interim("first draft")
        runCurrent()
        assertEquals("first draft", target.value.text)
        delivery.interim("old correction")
        delivery.interim("new correction")
        delivery.finalSegment("Final.")
        delivery.finalSegment(" Next.")
        delivery.beginDrain()
        delivery.finish()
        assertEquals("Final. Next.", target.value.text)
        assertEquals(listOf("first draft", "Final.", "Final. Next."), target.writes)
        delivery.finalSegment("stale")
        advanceUntilIdle()
        assertEquals("Final. Next.", target.value.text)
    }

    @Test
    fun `cancel discards queued finals and old worker cannot clean a newer session`() = runTest {
        val target = Target()
        val old = ProvisionalPasteDelivery(this, ProvisionalPasteSession(target))
        old.interim("draft")
        runCurrent()
        old.finalSegment("discard")
        old.cancel()
        assertEquals("", target.value.text)
        val next = ProvisionalPasteDelivery(this, ProvisionalPasteSession(target))
        next.finalSegment("New.")
        old.finalSegment("stale")
        next.finish()
        advanceUntilIdle()
        assertEquals("New.", target.value.text)
        assertEquals(listOf("draft", "", "New."), target.writes)
    }

    @Test
    fun `overflow cleans only the existing verifiable provisional tail`() = runTest {
        val target = Target()
        val delivery = ProvisionalPasteDelivery(this, ProvisionalPasteSession(target))
        delivery.interim("draft")
        runCurrent()
        repeat(129) { delivery.finalSegment("discard") }
        advanceUntilIdle()
        assertEquals("", target.value.text)
        assertEquals(listOf("draft", ""), target.writes)
        delivery.finalSegment("stale")
        advanceUntilIdle()
        assertEquals(2, target.writes.size)
    }

    private class Target : ProvisionalPasteTarget {
        override val replaceable = true
        var value = PasteSnapshot("", 0, 0)
        val writes = mutableListOf<String>()
        override fun snapshot() = value
        override fun replace(expected: PasteSnapshot, replacement: PasteSnapshot): Boolean {
            assertEquals(value, expected)
            value = replacement
            writes.add(value.text)
            return true
        }
    }
}
