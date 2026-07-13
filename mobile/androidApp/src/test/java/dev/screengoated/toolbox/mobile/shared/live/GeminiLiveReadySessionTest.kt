package dev.screengoated.toolbox.mobile.shared.live

import kotlinx.coroutines.async
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.yield
import okio.ByteString.Companion.encodeUtf8
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test

class GeminiLiveReadySessionTest {
    @Test
    fun `connected typestate sends nothing before one-time activation`() = runTest {
        val connector = FakeConnector()
        val connected = openGeminiLiveConnectedSession(connector)

        assertEquals(GeminiLiveSessionPhase.AWAITING_SETUP, connected.phase)
        assertTrue(connector.socket.sentPayloads.isEmpty())

        val ready = connected.activate(SETUP_PAYLOAD)
        assertEquals(listOf(SETUP_PAYLOAD), connector.socket.sentPayloads)
        assertEquals(GeminiLiveSessionPhase.ACTIVE, ready.phase)

        try {
            connected.activate(SETUP_PAYLOAD)
            fail("Expected one-time activation to reject a second setup")
        } catch (_: IllegalStateException) {
        }
        connected.close()
        assertEquals(0, connector.socket.closeCount)

        ready.close()
        assertEquals(1, connector.socket.closeCount)
        assertEquals(1, connector.socket.cancelCount)
    }

    @Test
    fun `connected session is returned only after websocket open`() = runTest {
        val connector = FakeConnector(openImmediately = false)
        val opening = async { openGeminiLiveConnectedSession(connector) }
        yield()

        assertFalse(opening.isCompleted)
        assertTrue(connector.socket.sentPayloads.isEmpty())

        connector.emitOpen()
        val connected = opening.await()
        assertEquals(GeminiLiveSessionPhase.AWAITING_SETUP, connected.phase)
        connected.close()
    }

    @Test
    fun `stale connected session fails activation without sending setup`() = runTest {
        val connector = FakeConnector()
        val connected = openGeminiLiveConnectedSession(connector)
        connector.listener.onClosed(1001, "idle transport expired")

        val error = expectSessionException {
            connected.activate(SETUP_PAYLOAD)
        }

        assertEquals(
            GeminiLiveSessionFailure.ClosedBeforeReady(1001, "idle transport expired"),
            error.failure,
        )
        assertTrue(connector.socket.sentPayloads.isEmpty())
        assertEquals(1, connector.socket.closeCount)
        assertEquals(1, connector.socket.cancelCount)
    }

    @Test
    fun `cancelling websocket open closes the transport once`() = runTest {
        val connector = FakeConnector(openImmediately = false)
        val opening = async { openGeminiLiveConnectedSession(connector) }
        yield()

        opening.cancelAndJoin()

        assertEquals(1, connector.socket.closeCount)
        assertEquals(1, connector.socket.cancelCount)
    }

    @Test
    fun `open returns only after setup and parses text and binary frames identically`() = runTest {
        val connector = FakeConnector()
        val session = openGeminiLiveReadySession(connector, SETUP_PAYLOAD)

        assertEquals(GeminiLiveSessionPhase.ACTIVE, session.phase)
        assertEquals(listOf(SETUP_PAYLOAD), connector.socket.sentPayloads)
        assertEquals(GeminiLiveReceiveResult.TimedOut, session.receive(timeoutMs = 1L))

        connector.emitText(SERVER_FRAME)
        connector.emitBinary(SERVER_FRAME)

        val text = session.receive() as GeminiLiveReceiveResult.Frame
        val binary = session.receive() as GeminiLiveReceiveResult.Frame
        assertEquals(GeminiLiveWireFormat.TEXT, text.wireFormat)
        assertEquals(GeminiLiveWireFormat.BINARY, binary.wireFormat)
        assertEquals("source", text.frame.inputTranscript)
        assertEquals(text.frame, binary.frame)

        session.close()
        assertEquals(1, connector.socket.closeCount)
        assertEquals(1, connector.socket.cancelCount)
    }

    @Test
    fun `media cannot be sent until a delayed setup acknowledgement makes the session ready`() = runTest {
        val connector = FakeConnector(setupReply = null)
        val opening = async { openGeminiLiveReadySession(connector, SETUP_PAYLOAD) }
        yield()

        assertFalse(opening.isCompleted)
        assertEquals(listOf(SETUP_PAYLOAD), connector.socket.sentPayloads)

        connector.emitText("""{"setupComplete":{}}""")
        val session = opening.await()
        assertTrue(session.trySend(AUDIO_PAYLOAD))
        assertEquals(listOf(SETUP_PAYLOAD, AUDIO_PAYLOAD), connector.socket.sentPayloads)

        session.close()
    }

    @Test
    fun `pre-ack observations and combined setup content retain wire order`() = runTest {
        val connector = FakeConnector(setupReply = null)
        val opening = async { openGeminiLiveReadySession(connector, SETUP_PAYLOAD) }
        yield()

        connector.emitText("""{"serverContent":{"inputTranscription":{"text":"before"}}}""")
        connector.emitBinary("not-json")
        connector.emitText(
            """{"setupComplete":{},"serverContent":{"modelTurn":{"parts":[{"text":"ready"}]}}}""",
        )
        val session = opening.await()

        val before = session.receive() as GeminiLiveReceiveResult.Frame
        val unparsed = session.receive() as GeminiLiveReceiveResult.Unparsed
        val combined = session.receive() as GeminiLiveReceiveResult.Frame
        assertEquals(GeminiLiveWireFormat.TEXT, before.wireFormat)
        assertEquals("before", before.frame.inputTranscript)
        assertEquals(GeminiLiveWireFormat.BINARY, unparsed.wireFormat)
        assertEquals("not-json", unparsed.payload)
        assertEquals(GeminiLiveWireFormat.TEXT, combined.wireFormat)
        assertTrue(combined.frame.setupComplete)
        assertEquals(listOf("ready"), combined.frame.visibleTextParts)
        assertEquals(GeminiLiveReceiveResult.TimedOut, session.receive(timeoutMs = 1L))

        session.close()
    }

    @Test
    fun `active send rejection is terminal and observable`() = runTest {
        val connector = FakeConnector(acceptActive = false)
        val session = openGeminiLiveReadySession(connector, SETUP_PAYLOAD)

        assertFalse(session.trySend(AUDIO_PAYLOAD))
        val terminal = session.receive() as GeminiLiveReceiveResult.Failed
        assertEquals(GeminiLiveSessionFailure.ActiveSendRejected, terminal.failure)
        assertEquals(GeminiLiveSessionPhase.CLOSED, session.phase)
        assertEquals(1, connector.socket.closeCount)
        assertEquals(1, connector.socket.cancelCount)
    }

    @Test
    fun `server error wins over setup complete and closes once`() = runTest {
        val connector = FakeConnector(
            setupReply = """{"setupComplete":{},"error":{"message":"denied"}}""",
        )

        val error = expectSessionException {
            openGeminiLiveReadySession(connector, SETUP_PAYLOAD)
        }

        assertEquals(GeminiLiveSessionFailure.Server("denied"), error.failure)
        assertEquals(1, connector.socket.closeCount)
        assertEquals(1, connector.socket.cancelCount)
    }

    @Test
    fun `transient setup server error preserves retryability`() = runTest {
        val connector = FakeConnector(
            setupReply =
                """{"error":{"code":503,"status":"UNAVAILABLE","message":"retry"}}""",
        )

        val error = expectSessionException {
            openGeminiLiveReadySession(connector, SETUP_PAYLOAD)
        }

        assertEquals(
            GeminiLiveSessionFailure.Server("retry", retryable = true),
            error.failure,
        )
    }

    @Test
    fun `setup send rejection fails immediately and closes once`() = runTest {
        val connector = FakeConnector(acceptSetup = false)

        val error = expectSessionException {
            openGeminiLiveReadySession(connector, SETUP_PAYLOAD)
        }

        assertEquals(GeminiLiveSessionFailure.SetupSendRejected, error.failure)
        assertEquals(1, connector.socket.closeCount)
        assertEquals(1, connector.socket.cancelCount)
    }

    @Test
    fun `failure callback before socket attachment still closes once`() = runTest {
        val transportError = IllegalStateException("connect failed")
        val connector = FakeConnector(connectFailure = transportError)

        val error = expectSessionException {
            openGeminiLiveReadySession(connector, SETUP_PAYLOAD)
        }

        val failure = error.failure as GeminiLiveSessionFailure.Transport
        assertEquals(transportError, failure.cause)
        assertEquals(1, connector.socket.closeCount)
        assertEquals(1, connector.socket.cancelCount)
    }

    @Test
    fun `terminal websocket callbacks are deduplicated`() = runTest {
        val connector = FakeConnector()
        val session = openGeminiLiveReadySession(connector, SETUP_PAYLOAD)

        connector.listener.onClosing(1001, "going away")
        connector.listener.onClosed(1001, "going away")
        connector.listener.onFailure(IllegalStateException("late failure"))

        val terminal = session.receive() as GeminiLiveReceiveResult.Closed
        val repeatedTerminal = session.receive(timeoutMs = 1L) as GeminiLiveReceiveResult.Closed
        assertEquals(1001, terminal.code)
        assertEquals("going away", terminal.reason)
        assertEquals(terminal, repeatedTerminal)
        assertEquals(GeminiLiveSessionPhase.CLOSED, session.phase)
        assertEquals(1, connector.socket.closeCount)
        assertEquals(1, connector.socket.cancelCount)
    }

    @Test
    fun `cancelling setup wait closes the transport once`() = runTest {
        val connector = FakeConnector(setupReply = null)
        val opening = async {
            openGeminiLiveReadySession(connector, SETUP_PAYLOAD)
        }
        yield()

        assertTrue(connector.socket.sentPayloads.contains(SETUP_PAYLOAD))
        assertFalse(opening.isCompleted)
        opening.cancelAndJoin()

        assertEquals(1, connector.socket.closeCount)
        assertEquals(1, connector.socket.cancelCount)
    }

    @Test
    fun `cancelling an active receive closes the transport once`() = runTest {
        val connector = FakeConnector()
        val session = openGeminiLiveReadySession(connector, SETUP_PAYLOAD)
        val receiving = async { session.receive() }
        yield()

        receiving.cancelAndJoin()

        assertEquals(GeminiLiveSessionPhase.CLOSED, session.phase)
        assertEquals(1, connector.socket.closeCount)
        assertEquals(1, connector.socket.cancelCount)
    }

    private suspend fun expectSessionException(
        block: suspend () -> Unit,
    ): GeminiLiveSessionException {
        return try {
            block()
            fail("Expected GeminiLiveSessionException")
            error("unreachable")
        } catch (error: GeminiLiveSessionException) {
            error
        }
    }

    private class FakeConnector(
        private val acceptSetup: Boolean = true,
        private val acceptActive: Boolean = true,
        private val setupReply: String? = """{"setupComplete":{}}""",
        private val connectFailure: Throwable? = null,
        private val openImmediately: Boolean = true,
    ) : GeminiLiveSocketConnector {
        lateinit var listener: GeminiLiveSocketListener
        lateinit var socket: FakeSocket

        override fun connect(listener: GeminiLiveSocketListener): GeminiLiveSocket {
            this.listener = listener
            socket = FakeSocket { payload ->
                if (payload == SETUP_PAYLOAD) {
                    if (!acceptSetup) {
                        return@FakeSocket false
                    }
                    setupReply?.let(listener::onText)
                    return@FakeSocket true
                }
                acceptActive
            }
            if (connectFailure == null && openImmediately) {
                listener.onOpen()
            } else {
                connectFailure?.let(listener::onFailure)
            }
            return socket
        }

        fun emitOpen() {
            listener.onOpen()
        }

        fun emitText(payload: String) {
            listener.onText(payload)
        }

        fun emitBinary(payload: String) {
            listener.onBinary(payload.encodeUtf8())
        }
    }

    private class FakeSocket(
        private val sendResult: (String) -> Boolean,
    ) : GeminiLiveSocket {
        val sentPayloads = mutableListOf<String>()
        var closeCount = 0
            private set
        var cancelCount = 0
            private set

        override fun send(payload: String): Boolean {
            sentPayloads += payload
            return sendResult(payload)
        }

        override fun close(code: Int, reason: String): Boolean {
            closeCount++
            return true
        }

        override fun cancel() {
            cancelCount++
        }
    }

    private companion object {
        private const val SETUP_PAYLOAD = """{"setup":{"model":"models/test"}}"""
        private const val AUDIO_PAYLOAD = """{"realtimeInput":{"audio":{"data":"AA=="}}}"""
        private const val SERVER_FRAME =
            """{"serverContent":{"inputTranscription":{"text":"source"},"turnComplete":true}}"""
    }
}
