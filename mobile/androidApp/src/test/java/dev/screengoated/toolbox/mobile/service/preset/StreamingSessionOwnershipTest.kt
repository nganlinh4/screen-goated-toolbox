package dev.screengoated.toolbox.mobile.service.preset

import dev.screengoated.toolbox.mobile.preset.AudioStreamingSession
import dev.screengoated.toolbox.mobile.preset.AudioStreamingTranscriptResult
import java.io.IOException
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.cancel
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class StreamingSessionOwnershipTest {
    private class Session : AudioStreamingSession {
        var cancellations = 0
        override suspend fun appendPcm16Chunk(chunk: ShortArray) = Unit
        override suspend fun finish() = AudioStreamingTranscriptResult("final")
        override fun cancel() { cancellations++ }
    }

    @Test
    fun `cancelled dispatcher return closes acquired session`() = runTest {
        val session = Session()
        var installed = false
        try {
            acquireStreamingSession(
                acquire = { currentCoroutineContext().cancel(); session },
                install = { installed = true },
                dispatcher = StandardTestDispatcher(testScheduler),
            )
        } catch (_: CancellationException) { }
        assertEquals(1, session.cancellations)
        assertTrue(!installed)
    }

    @Test
    fun `failed initial flush closes session but successful installation transfers ownership`() = runTest {
        val failed = Session()
        try {
            acquireStreamingSession({ failed }, { throw IOException("flush rejected") }, StandardTestDispatcher(testScheduler))
        } catch (_: IOException) { }
        assertEquals(1, failed.cancellations)
        val accepted = Session()
        acquireStreamingSession({ accepted }, {}, StandardTestDispatcher(testScheduler))
        assertEquals(0, accepted.cancellations)
    }

    @Test
    fun `finalization failure returns WAV fallback without resetting paste suppression`() = runTest {
        val paste = StreamingPasteState()
        paste.accept(paste.generation)
        paste.recordInsertion(true)
        var reported = false
        val result = finalizeStreamingOrFallback({ throw IOException("end rejected") }, { reported = true })
        assertNull(result)
        assertTrue(reported)
        assertTrue(paste.suppressFinalPaste)
        assertEquals("final", finalizeStreamingOrFallback({ AudioStreamingTranscriptResult("final") }, {})?.transcript)
    }

    @Test
    fun `cancellation does not become a batch submission`() = runTest {
        var cancelled = false
        var reported = false
        try {
            finalizeStreamingOrFallback({ throw CancellationException() }, { reported = true })
        } catch (_: CancellationException) { cancelled = true }
        assertTrue(cancelled)
        assertTrue(!reported)
    }
}
