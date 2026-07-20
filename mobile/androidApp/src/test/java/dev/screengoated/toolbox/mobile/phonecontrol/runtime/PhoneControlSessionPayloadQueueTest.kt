package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class PhoneControlSessionPayloadQueueTest {
    @Test
    fun `rejected send retains one FIFO head for resumable retry`() {
        val queue = PhoneControlSessionPayloadQueue(
            maximumCount = 3,
            maximumUtf8Bytes = 32,
            maximumPayloadUtf8Bytes = 16,
        )
        assertTrue(queue.offer("owner", PhoneControlOutboundKind.TOOL_RESPONSE))
        assertTrue(queue.offer("held", PhoneControlOutboundKind.TOOL_SCREEN_EVIDENCE))
        assertTrue(queue.prepareReconnect("resume-token"))

        val owner = queue.next()
        assertEquals("owner", owner?.payload)
        assertTrue(owner === queue.next())
        queue.markSent(requireNotNull(owner))
        assertEquals("held", queue.next()?.payload)
        assertEquals(1, queue.snapshot().count)
    }

    @Test
    fun `count and UTF8 byte limits latch overflow until session abandonment`() {
        val countBound = PhoneControlSessionPayloadQueue(2, 32, 16)
        assertTrue(countBound.offer("one", PhoneControlOutboundKind.TOOL_RESPONSE))
        assertTrue(countBound.offer("two", PhoneControlOutboundKind.TOOL_RESPONSE))
        assertFalse(countBound.offer("three", PhoneControlOutboundKind.TOOL_RESPONSE))
        assertTrue(countBound.snapshot().overflowed)
        assertFalse(countBound.offer("later", PhoneControlOutboundKind.TOOL_RESPONSE))

        val byteBound = PhoneControlSessionPayloadQueue(3, 7, 6)
        assertTrue(byteBound.offer("éé", PhoneControlOutboundKind.TOOL_RESPONSE)) // Four UTF-8 bytes.
        assertFalse(byteBound.offer("éé", PhoneControlOutboundKind.TOOL_RESPONSE))
        assertEquals(4, byteBound.snapshot().utf8Bytes)
        assertTrue(byteBound.snapshot().overflowed)
    }

    @Test
    fun `single oversized payload is rejected without retention`() {
        val queue = PhoneControlSessionPayloadQueue(2, 16, 5)

        assertFalse(queue.offer("123456", PhoneControlOutboundKind.TOOL_RESPONSE))
        assertNull(queue.next())
        assertEquals(0, queue.snapshot().count)
        assertTrue(queue.snapshot().overflowed)
    }

    @Test
    fun `nonresumable reconnect cannot leak prior session payloads`() {
        val queue = PhoneControlSessionPayloadQueue()
        assertTrue(queue.offer("owner-receipt", PhoneControlOutboundKind.TOOL_RESPONSE))
        assertFalse(queue.prepareReconnect(" \t "))

        assertNull(queue.next())
        assertTrue(queue.offer("fresh-session-input", PhoneControlOutboundKind.TOOL_RESPONSE))
        assertEquals("fresh-session-input", queue.next()?.payload)
        assertFalse(queue.snapshot().overflowed)
    }

    @Test
    fun `only bounded nonblank resumption handles preserve FIFO`() {
        val valid = "resume-token"
        assertEquals(valid, PhoneControlResumptionPolicy.usableHandle(valid))
        assertNull(
            PhoneControlResumptionPolicy.usableHandle(
                "x".repeat(PhoneControlResumptionPolicy.MAXIMUM_HANDLE_UTF8_BYTES + 1),
            ),
        )
    }
}
