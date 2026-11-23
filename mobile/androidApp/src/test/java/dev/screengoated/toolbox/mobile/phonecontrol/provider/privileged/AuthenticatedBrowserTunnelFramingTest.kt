package dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged

import java.io.ByteArrayOutputStream
import java.io.InputStream
import java.nio.charset.StandardCharsets
import java.util.concurrent.atomic.AtomicBoolean
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class AuthenticatedBrowserTunnelFramingTest {
    @Test
    fun aSwitchingStatusLineSplitAcrossReadsIsRecognizedAndNothingAfterItIsConsumed() {
        val upstream = ChunkedInput("HTTP/1.1 10", "1 Switching Protocols\r", "\nUpgrade: websocket\r\n\r\n", "frame")

        val statusLine = readTunnelStatusLine(upstream)

        assertEquals("HTTP/1.1 101 Switching Protocols\r\n", text(statusLine))
        assertTrue(isSwitchingProtocols(statusLine))
        assertEquals("Upgrade: websocket\r\n\r\nframe", text(upstream.readBytes()))
    }

    @Test
    fun theBodyBoundaryForwardsEachByteOnceAndStopsBeforeAnUnauthenticatedRequest() {
        val output = ByteArrayOutputStream()
        forwardTunnelClientStream(ChunkedInput("{", "}GET /json/list"), output, 2L, AtomicBoolean(false))
        assertEquals("{}", text(output.toByteArray()))
    }

    @Test
    fun anUpgradedConnectionForwardsTheFirstPostBodyByteExactlyOnce() {
        val output = ByteArrayOutputStream()
        forwardTunnelClientStream(ChunkedInput("{}", "f", "rame"), output, 2L, AtomicBoolean(true))
        assertEquals("{}frame", text(output.toByteArray()))
    }

    @Test
    fun upgradedFramesReachABufferedUpstreamBeforeTheNextRead() {
        val upstream = FlushOnlyOutput()
        val deliveredAtRead = mutableListOf<String>()
        val client = ChunkedInput("{}", "frame-1", "frame-2") { deliveredAtRead += upstream.delivered() }

        forwardTunnelClientStream(client, upstream, 2L, AtomicBoolean(true))

        // Each read may block on a live socket, so everything relayed before it must already be flushed.
        assertEquals(listOf("", "{}", "{}f", "{}frame-1", "{}frame-1frame-2"), deliveredAtRead)
        assertEquals("{}frame-1frame-2", upstream.delivered())
    }

    @Test
    fun theDownstreamCopyFlushesEveryChunk() {
        val client = FlushOnlyOutput()
        val deliveredAtRead = mutableListOf<String>()
        val upstream = ChunkedInput("frame-1", "frame-2") { deliveredAtRead += client.delivered() }

        copyTunnelStreamFlushing(upstream, client)

        assertEquals(listOf("", "frame-1", "frame-1frame-2"), deliveredAtRead)
    }

    /** Models a buffered stream such as the ADB upstream: bytes leave only on flush. */
    private class FlushOnlyOutput : java.io.OutputStream() {
        private val pending = ByteArrayOutputStream()
        private val sent = ByteArrayOutputStream()

        override fun write(byte: Int) = pending.write(byte)

        override fun write(buffer: ByteArray, off: Int, len: Int) = pending.write(buffer, off, len)

        override fun flush() {
            pending.writeTo(sent)
            pending.reset()
        }

        fun delivered(): String = sent.toByteArray().toString(StandardCharsets.ISO_8859_1)
    }

    /** Serves each chunk from a separate read, like a socket that delivers a line in pieces. */
    private class ChunkedInput(
        vararg chunks: String,
        private val onRead: () -> Unit = {},
    ) : InputStream() {
        private val pending = ArrayDeque(chunks.map { it.toByteArray(StandardCharsets.ISO_8859_1) })
        private var offset = 0

        override fun read(): Int {
            onRead()
            val chunk = current() ?: return -1
            return chunk[offset++].toInt() and 0xFF
        }

        override fun read(buffer: ByteArray, off: Int, len: Int): Int {
            if (len == 0) return 0
            onRead()
            val chunk = current() ?: return -1
            val count = minOf(len, chunk.size - offset)
            chunk.copyInto(buffer, off, offset, offset + count)
            offset += count
            return count
        }

        private fun current(): ByteArray? {
            while (pending.isNotEmpty() && offset >= pending.first().size) {
                pending.removeFirst()
                offset = 0
            }
            return pending.firstOrNull()
        }
    }

    private fun text(bytes: ByteArray): String = bytes.toString(StandardCharsets.ISO_8859_1)
}
