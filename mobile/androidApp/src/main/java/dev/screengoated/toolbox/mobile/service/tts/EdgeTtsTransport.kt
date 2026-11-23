package dev.screengoated.toolbox.mobile.service.tts

import java.io.ByteArrayOutputStream
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okio.ByteString

internal const val EDGE_SETUP_SECONDS = 10L
internal const val EDGE_SESSION_SECONDS = 180L
internal const val EDGE_MAX_AUDIO_BYTES = 16 * 1024 * 1024
private const val EDGE_MAX_TEXT_CHARS = 64 * 1024

/** A cancelled DNS call retains its slot until OkHttp reports actual termination. */
internal class EdgeConnectionLease(private val onRelease: () -> Unit) {
    private val released = AtomicBoolean(false)
    fun release() { if (released.compareAndSet(false, true)) onRelease() }

    companion object {
        private val pool = EdgeConnectionPool(2)
        fun acquire(): EdgeConnectionLease? = pool.acquire()
    }
}

internal class EdgeConnectionPool(private val limit: Int) {
    private val active = AtomicInteger()
    fun acquire(): EdgeConnectionLease? {
        while (true) {
            val count = active.get()
            if (count >= limit) return null
            if (active.compareAndSet(count, count + 1)) {
                return EdgeConnectionLease { active.decrementAndGet() }
            }
        }
    }
}

internal sealed interface EdgeOutcome {
    data class Complete(val mp3: ByteArray) : EdgeOutcome
    data class Failure(val message: String) : EdgeOutcome
    data object Cancelled : EdgeOutcome
}

/** All callback observations and deadline checks linearize on this lock. */
internal class EdgeStreamState(private val clock: () -> Long = System::nanoTime) {
    private val lock = Any()
    private val wake = ArrayBlockingQueue<Unit>(1)
    private val started = clock()
    private val bytes = ByteArrayOutputStream()
    private var opened = false
    private var requestSent = false
    private var outcome: EdgeOutcome? = null

    private fun checkDeadline() {
        if (outcome != null) return
        val elapsed = clock() - started
        if (!opened && elapsed >= TimeUnit.SECONDS.toNanos(EDGE_SETUP_SECONDS)) {
            outcome = EdgeOutcome.Failure("Edge TTS connection timed out.")
        } else if (elapsed >= TimeUnit.SECONDS.toNanos(EDGE_SESSION_SECONDS)) {
            outcome = EdgeOutcome.Failure("Edge TTS synthesis timed out.")
        }
    }

    fun opened(): Unit = synchronized(lock) {
        checkDeadline()
        if (outcome == null) opened = true
        wake.offer(Unit)
    }

    fun binary(payload: ByteString): Unit = synchronized(lock) {
        checkDeadline()
        if (outcome == null && opened && requestSent) {
            if (payload.size < 2) {
                outcome = EdgeOutcome.Failure("Edge TTS returned a malformed audio frame.")
            } else {
                val headerLength = ((payload[0].toInt() and 0xff) shl 8) or (payload[1].toInt() and 0xff)
                val start = 2 + headerLength
                if (start > payload.size) {
                    outcome = EdgeOutcome.Failure("Edge TTS returned a malformed audio frame.")
                } else if (payload.substring(2, start).utf8().lineSequence().any { it.trimEnd('\r') == "Path:audio" }) {
                    val size = payload.size - start
                    if (size > EDGE_MAX_AUDIO_BYTES - bytes.size()) {
                        outcome = EdgeOutcome.Failure("Edge TTS audio exceeds the size limit.")
                    } else if (size > 0) {
                        bytes.write(payload.substring(start).toByteArray())
                    }
                }
            }
        }
        wake.offer(Unit)
    }

    fun text(text: String): Unit = synchronized(lock) {
        checkDeadline()
        if (outcome == null && opened && requestSent) {
            if (text.length > EDGE_MAX_TEXT_CHARS) {
                outcome = EdgeOutcome.Failure("Edge TTS response exceeds the size limit.")
            } else {
                val separator = text.indexOf("\r\n\r\n")
                val header = if (separator >= 0) text.substring(0, separator) else null
                if (header == null) {
                    outcome = EdgeOutcome.Failure("Edge TTS returned a malformed response.")
                } else if (header.lineSequence().any { it.trimEnd('\r') == "Path:turn.end" }) {
                    outcome = if (bytes.size() == 0) EdgeOutcome.Failure("Edge TTS returned no audio.")
                    else EdgeOutcome.Complete(bytes.toByteArray())
                }
            }
        }
        wake.offer(Unit)
    }

    fun fail(message: String): Unit = synchronized(lock) {
        checkDeadline()
        if (outcome == null) outcome = EdgeOutcome.Failure(message)
        wake.offer(Unit)
    }

    fun cancel(): Unit = synchronized(lock) {
        if (outcome == null) outcome = EdgeOutcome.Cancelled
        wake.offer(Unit)
    }

    fun sendIfActive(requestPayload: Boolean, send: () -> Boolean): Boolean? = synchronized(lock) {
        checkDeadline()
        if (outcome != null || !opened) return@synchronized null
        if (requestPayload) requestSent = true
        send()
    }

    fun currentOutcome(): EdgeOutcome? = synchronized(lock) { checkDeadline(); outcome }

    fun awaitOpen(isStale: () -> Boolean): EdgeOutcome? {
        while (true) {
            if (isStale()) { cancel(); return EdgeOutcome.Cancelled }
            synchronized(lock) {
                checkDeadline()
                outcome?.let { return it }
                if (opened) return null
            }
            wake.poll(50, TimeUnit.MILLISECONDS)
        }
    }

    fun awaitResult(isStale: () -> Boolean): EdgeOutcome {
        while (true) {
            if (isStale()) { cancel(); return EdgeOutcome.Cancelled }
            synchronized(lock) {
                checkDeadline()
                outcome?.let { return it }
            }
            wake.poll(50, TimeUnit.MILLISECONDS)
        }
    }
}

internal class EdgeSocketSession(
    client: OkHttpClient,
    request: Request,
    lease: EdgeConnectionLease,
) : AutoCloseable {
    val state = EdgeStreamState()
    private val socket: WebSocket

    init {
        try {
            socket = client.newWebSocket(request, object : WebSocketListener() {
                override fun onOpen(webSocket: WebSocket, response: Response) = state.opened()
                override fun onMessage(webSocket: WebSocket, text: String) = state.text(text)
                override fun onMessage(webSocket: WebSocket, bytes: ByteString) = state.binary(bytes)
                override fun onClosing(webSocket: WebSocket, code: Int, reason: String) = state.fail("Edge TTS closed before completion.")
                override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                    state.fail("Edge TTS closed before completion.")
                    lease.release()
                }
                override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                    state.fail("Edge TTS connection failed.")
                    lease.release()
                }
            })
        } catch (error: Throwable) {
            lease.release()
            throw error
        }
    }

    fun send(payload: String): Boolean = socket.send(payload)
    override fun close() { state.cancel(); socket.cancel() }
}
