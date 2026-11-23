package dev.screengoated.toolbox.mobile.service.tts

import java.io.File
import java.net.InetAddress
import java.net.ServerSocket
import java.nio.charset.StandardCharsets
import java.security.MessageDigest
import java.util.Base64
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicReference
import kotlin.concurrent.thread
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import okio.ByteString.Companion.toByteString
import okhttp3.OkHttpClient
import okhttp3.Request
import org.junit.Assert.*
import org.junit.Test

class EdgeTtsTransportTest {
    private fun audio(size: Int = 3): okio.ByteString {
        val header = "Path:audio\r\n".toByteArray()
        return (byteArrayOf((header.size shr 8).toByte(), header.size.toByte()) + header + ByteArray(size) { 1 }).toByteString()
    }

    @Test fun sharedFixtureControlsCompletionCancellationAndBounds() {
        val fixture = listOf(
            File("../../parity-fixtures/tts-runtime/edge-transport.json"),
            File("../parity-fixtures/tts-runtime/edge-transport.json"),
            File("parity-fixtures/tts-runtime/edge-transport.json"),
        ).first(File::isFile).readText()
        val root = Json.parseToJsonElement(fixture).jsonObject
        val limits = root.getValue("limits").jsonObject
        assertEquals(EDGE_SETUP_SECONDS.toInt(), limits.getValue("setup_seconds").jsonPrimitive.int)
        assertEquals(EDGE_SESSION_SECONDS.toInt(), limits.getValue("session_seconds").jsonPrimitive.int)
        assertEquals(EDGE_MAX_AUDIO_BYTES, limits.getValue("encoded_audio_bytes").jsonPrimitive.int)
        for (case in root.getValue("cases").jsonArray.map { it.jsonObject }) {
            var now = 0L
            val state = EdgeStreamState { now }
            val events = case.getValue("events").jsonArray.map { it.jsonPrimitive.content }
            if (events.first() !in setOf("cancel", "setup_timeout")) {
                state.opened()
                assertEquals(true, state.sendIfActive(true) { true })
            }
            for (event in events) when (event) {
                "audio" -> state.binary(audio())
                "audio_overflow" -> state.binary(audio(EDGE_MAX_AUDIO_BYTES + 1))
                "turn_end" -> state.text("Path:turn.end\r\n\r\n")
                "malformed_audio" -> state.binary(byteArrayOf(0, 4, 1, 2).toByteString())
                "unterminated_turn" -> state.text("Path:turn.end")
                "close" -> state.fail("closed")
                "error" -> state.fail("failed")
                "cancel" -> state.cancel()
                "setup_timeout" -> now = TimeUnit.SECONDS.toNanos(EDGE_SETUP_SECONDS)
                "session_timeout" -> now = TimeUnit.SECONDS.toNanos(EDGE_SESSION_SECONDS)
                else -> error("unknown fixture event: $event")
            }
            val outcome = state.currentOutcome()
            val actual = when (outcome) {
                is EdgeOutcome.Complete -> "complete"
                is EdgeOutcome.Failure -> "failure"
                EdgeOutcome.Cancelled -> "cancelled"
                null -> "pending"
            }
            assertEquals(case.getValue("name").jsonPrimitive.content, case.getValue("expected").jsonPrimitive.content, actual)
        }
    }

    @Test fun contentCannotForgeCompletionAndLateCallbacksCannotReviveRetiredRequest() {
        val state = EdgeStreamState()
        state.opened()
        state.binary(audio())
        state.text("Path:turn.end\r\n\r\n")
        assertNull(state.currentOutcome())
        assertEquals(true, state.sendIfActive(true) { true })
        state.binary(audio())
        state.text("Path:audio.metadata\r\n\r\nPath:turn.end")
        assertNull(state.currentOutcome())
        state.cancel()
        state.text("Path:turn.end\r\n\r\n")
        assertEquals(EdgeOutcome.Cancelled, state.currentOutcome())
    }

    @Test fun cancellationInterruptsOpenWaitWithoutWaitingForCallback() {
        val state = EdgeStreamState()
        val stale = AtomicBoolean(false)
        var outcome: EdgeOutcome? = null
        val waiter = thread { outcome = state.awaitOpen { stale.get() } }
        stale.set(true)
        waiter.join(1000)
        assertFalse(waiter.isAlive)
        assertEquals(EdgeOutcome.Cancelled, outcome)
    }

    @Test fun unterminatedHeadersAndMalformedBinaryCannotComplete() {
        fun active(): EdgeStreamState = EdgeStreamState().also {
            it.opened()
            assertEquals(true, it.sendIfActive(true) { true })
        }
        val missingSeparator = active()
        missingSeparator.binary(audio())
        missingSeparator.text("Path:turn.end")
        assertTrue(missingSeparator.currentOutcome() is EdgeOutcome.Failure)
        for (payload in listOf(byteArrayOf(0), byteArrayOf(0, 4, 1, 2))) {
            val malformed = active()
            malformed.binary(payload.toByteString())
            malformed.text("Path:turn.end\r\n\r\n")
            assertTrue(malformed.currentOutcome() is EdgeOutcome.Failure)
        }
    }

    @Test fun leaseCapacityRemainsHeldUntilActualTerminalCallback() {
        val pool = EdgeConnectionPool(2)
        val first = pool.acquire()!!
        val second = pool.acquire()!!
        try {
            assertNull(pool.acquire())
            first.release()
            first.release()
            val replacement = pool.acquire()
            assertNotNull(replacement)
            replacement!!.release()
        } finally {
            first.release()
            second.release()
        }
    }

    @Test fun loopbackWebSocketDeliversCompleteAudioOnlyAfterTurnEnd() {
        val server = ServerSocket(0, 1, InetAddress.getByName("127.0.0.1")).apply { soTimeout = 5000 }
        val proceed = CountDownLatch(1)
        val serverFailure = AtomicReference<Throwable?>(null)
        val serverThread = thread {
            try {
                server.accept().use { socket ->
                    socket.soTimeout = 5000
                    val reader = socket.getInputStream().bufferedReader(StandardCharsets.US_ASCII)
                    var key: String? = null
                    while (true) {
                        val line = reader.readLine() ?: error("missing handshake")
                        if (line.isEmpty()) break
                        if (line.startsWith("Sec-WebSocket-Key:", ignoreCase = true)) key = line.substringAfter(':').trim()
                    }
                    val accept = Base64.getEncoder().encodeToString(MessageDigest.getInstance("SHA-1").digest(
                        (requireNotNull(key) + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11").toByteArray(StandardCharsets.US_ASCII),
                    ))
                    val output = socket.getOutputStream()
                    output.write("HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: $accept\r\n\r\n".toByteArray(StandardCharsets.US_ASCII))
                    output.flush()
                    check(proceed.await(5, TimeUnit.SECONDS))
                    fun frame(opcode: Int, bytes: ByteArray) {
                        check(bytes.size < 126)
                        output.write(byteArrayOf((0x80 or opcode).toByte(), bytes.size.toByte()))
                        output.write(bytes)
                    }
                    frame(2, audio().toByteArray())
                    frame(1, "Path:turn.end\r\n\r\n".toByteArray())
                    output.flush()
                    Thread.sleep(100)
                }
            } catch (error: Throwable) { serverFailure.set(error) }
        }
        val client = OkHttpClient()
        val lease = requireNotNull(EdgeConnectionLease.acquire())
        val session = EdgeSocketSession(client, Request.Builder().url("ws://127.0.0.1:${server.localPort}/edge").build(), lease)
        try {
            assertNull(session.state.awaitOpen { false })
            assertEquals(true, session.state.sendIfActive(true) { session.send("request") })
            proceed.countDown()
            val result = session.state.awaitResult { false }
            assertTrue(result is EdgeOutcome.Complete)
            assertArrayEquals(byteArrayOf(1, 1, 1), (result as EdgeOutcome.Complete).mp3)
        } finally {
            session.close()
            server.close()
            serverThread.join(5000)
            client.dispatcher.executorService.shutdownNow()
            client.connectionPool.evictAll()
        }
        assertFalse(serverThread.isAlive)
        assertNull(serverFailure.get())
    }
}
