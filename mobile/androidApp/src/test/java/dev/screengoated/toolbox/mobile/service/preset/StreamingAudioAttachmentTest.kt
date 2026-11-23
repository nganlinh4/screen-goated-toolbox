package dev.screengoated.toolbox.mobile.service.preset

import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import dev.screengoated.toolbox.mobile.preset.PresetAudioCodec

class StreamingAudioAttachmentTest {
    @get:Rule val temporary = TemporaryFolder()

    @Test fun longSessionOmitsAttachmentAndDeletesSpool() {
        val attachment = StreamingAudioAttachment(temporary.root, 8)
        assertEquals(AudioAttachmentAppend.APPENDED, attachment.append(shortArrayOf(1,2,3,4), true))
        assertEquals(AudioAttachmentAppend.APPENDED, attachment.append(shortArrayOf(5,6,7,8), true))
        assertArrayEquals(shortArrayOf(1,2,3,4,5,6,7,8), PresetAudioCodec.decodePcm16MonoWav(attachment.toWavBytes()))
        assertEquals(AudioAttachmentAppend.OMITTED, attachment.append(shortArrayOf(9), true))
        repeat(10000) { assertEquals(AudioAttachmentAppend.OMITTED_CONTINUING, attachment.append(shortArrayOf(10), true)) }
        assertTrue(attachment.toWavBytes().isEmpty())
        assertTrue(temporary.root.listFiles().orEmpty().isEmpty())
        attachment.clear()
    }

    @Test fun finiteLimitKeepsAllAcceptedSamplesAndDeletesSpool() {
        val attachment = StreamingAudioAttachment(temporary.root, 4)
        assertEquals(AudioAttachmentAppend.APPENDED, attachment.append(shortArrayOf(Short.MIN_VALUE, -1, 0, Short.MAX_VALUE), false))
        assertEquals(AudioAttachmentAppend.LIMIT_REACHED, attachment.append(shortArrayOf(8), false))
        assertEquals(AudioAttachmentAppend.IGNORED, attachment.append(shortArrayOf(9), false))
        assertArrayEquals(shortArrayOf(Short.MIN_VALUE, -1, 0, Short.MAX_VALUE), PresetAudioCodec.decodePcm16MonoWav(attachment.toWavBytes()))
        attachment.clear()
        assertTrue(temporary.root.listFiles().orEmpty().isEmpty())
        assertEquals(AudioAttachmentAppend.IGNORED, attachment.append(shortArrayOf(10), false))
    }

    @Test fun excessChunkIsRejectedWholeAndCancellationClosesStorage() {
        val attachment = StreamingAudioAttachment(temporary.root, 4)
        attachment.append(shortArrayOf(1, 2, 3), false)
        assertEquals(AudioAttachmentAppend.LIMIT_REACHED, attachment.append(shortArrayOf(4, 5), false))
        assertArrayEquals(shortArrayOf(1, 2, 3), PresetAudioCodec.decodePcm16MonoWav(attachment.toWavBytes()))
        attachment.clear()
        assertTrue(temporary.root.listFiles().orEmpty().isEmpty())
        assertThrows(IllegalStateException::class.java) { attachment.toWavBytes() }
    }

    @Test fun invalidStorageDirectoryFailsBeforeCapture() {
        val notDirectory = temporary.newFile()
        assertThrows(java.io.IOException::class.java) { StreamingAudioAttachment(notDirectory) }
    }

    @Test fun liveFailureBeforeOverflowPreservesFallbackEvenForStaleLiveChunks() {
        val attachment = StreamingAudioAttachment(temporary.root, 4)
        attachment.append(shortArrayOf(1, 2, 3), true)
        assertTrue(attachment.fallBackToFinite())
        assertEquals(AudioAttachmentAppend.LIMIT_REACHED, attachment.append(shortArrayOf(4, 5), true))
        assertFalse(attachment.omitted)
        assertArrayEquals(shortArrayOf(1, 2, 3), PresetAudioCodec.decodePcm16MonoWav(attachment.toWavBytes()))
        attachment.clear()
    }

    @Test fun liveFailureAfterOmissionCannotClaimARecoverableFallback() {
        val attachment = StreamingAudioAttachment(temporary.root, 2)
        attachment.append(shortArrayOf(1, 2), true)
        assertEquals(AudioAttachmentAppend.OMITTED, attachment.append(shortArrayOf(3), true))
        assertFalse(attachment.fallBackToFinite())
        assertTrue(attachment.toWavBytes().isEmpty())
        attachment.clear()
    }
}
