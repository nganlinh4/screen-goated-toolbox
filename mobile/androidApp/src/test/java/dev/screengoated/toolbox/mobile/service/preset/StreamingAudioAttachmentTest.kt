package dev.screengoated.toolbox.mobile.service.preset

import org.junit.Assert.*
import org.junit.Test

class StreamingAudioAttachmentTest {
    @Test fun longSessionOmitsAttachmentOnceAndCanReset() {
        val attachment = StreamingAudioAttachment(8)
        assertFalse(attachment.append(shortArrayOf(1,2,3,4), true))
        assertFalse(attachment.append(shortArrayOf(5,6,7,8), true))
        assertArrayEquals(shortArrayOf(1,2,3,4,5,6,7,8), attachment.toShortArray())
        assertTrue(attachment.append(shortArrayOf(9), true))
        repeat(10000) { assertFalse(attachment.append(shortArrayOf(10), true)) }
        assertTrue(attachment.toShortArray().isEmpty())
        attachment.clear()
        assertFalse(attachment.omitted)
        attachment.append(shortArrayOf(11), true)
        assertArrayEquals(shortArrayOf(11), attachment.toShortArray())
    }
}
