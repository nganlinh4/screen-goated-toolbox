package dev.screengoated.toolbox.mobile.shared.live

import kotlinx.coroutines.channels.Channel
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okio.ByteString
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicReference
import java.io.IOException
import java.util.ArrayDeque

internal class GeminiLiveHttpFailure(val status: Int, cause: Throwable) : IOException("Gemini Live HTTP $status", cause)

internal fun interface GeminiLiveSocketConnector {
    fun connect(listener: GeminiLiveSocketListener): GeminiLiveSocket
}

internal interface GeminiLiveSocket {
    fun send(payload: String): Boolean

    fun close(code: Int, reason: String): Boolean

    fun cancel()
}

internal interface GeminiLiveSocketListener {
    fun onOpen()

    fun onText(payload: String)

    fun onBinary(payload: ByteString)

    fun onClosing(code: Int, reason: String)

    fun onClosed(code: Int, reason: String)

    fun onFailure(error: Throwable)
}

internal class OkHttpGeminiLiveSocketConnector(
    private val client: OkHttpClient,
    private val request: Request,
) : GeminiLiveSocketConnector {
    override fun connect(listener: GeminiLiveSocketListener): GeminiLiveSocket {
        val socket = client.newWebSocket(
            request,
            object : WebSocketListener() {
                override fun onOpen(webSocket: WebSocket, response: Response) = listener.onOpen()

                override fun onMessage(webSocket: WebSocket, text: String) = listener.onText(text)

                override fun onMessage(webSocket: WebSocket, bytes: ByteString) = listener.onBinary(bytes)

                override fun onClosing(webSocket: WebSocket, code: Int, reason: String) =
                    listener.onClosing(code, reason)

                override fun onClosed(webSocket: WebSocket, code: Int, reason: String) =
                    listener.onClosed(code, reason)

                override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) =
                    listener.onFailure(response?.let { GeminiLiveHttpFailure(it.code, t) } ?: t)
            },
        )
        return object : GeminiLiveSocket {
            override fun send(payload: String): Boolean = socket.send(payload)

            override fun close(code: Int, reason: String): Boolean = socket.close(code, reason)

            override fun cancel() = socket.cancel()
        }
    }
}

internal sealed interface GeminiLiveCoreEvent {
    data object Opened : GeminiLiveCoreEvent

    data class Frame(
        val frame: GeminiLiveServerFrame,
        val wireFormat: GeminiLiveWireFormat,
    ) : GeminiLiveCoreEvent

    data class Unparsed(
        val payload: String,
        val wireFormat: GeminiLiveWireFormat,
    ) : GeminiLiveCoreEvent

    data class Closed(
        val code: Int?,
        val reason: String?,
    ) : GeminiLiveCoreEvent

    data class Failed(
        val failure: GeminiLiveSessionFailure,
    ) : GeminiLiveCoreEvent
}

private data class QueuedGeminiLiveEvent(
    val event: GeminiLiveCoreEvent,
    val payloadBytes: Int = 0,
    val payload: Boolean = false,
)

internal class GeminiLiveSessionCore(
    private val parseFrame: (String) -> GeminiLiveServerFrame? = ::parseGeminiLiveServerFrame,
) {
    private val setupBudgetLock = Any()
    private var setupFrames = 0
    private var setupBytes = 0
    private var setupAckQueued = false
    private var queuedFrames = 0
    private var queuedBytes = 0
    private var activeBufferOverflow = false
    private val state = AtomicReference(GeminiLiveSessionPhase.OPENING)
    private val socket = AtomicReference<GeminiLiveSocket?>(null)
    private val terminal = AtomicReference<GeminiLiveCoreEvent?>(null)
    private val shutdownRequested = AtomicBoolean(false)
    private val socketShutdown = AtomicBoolean(false)
    private val events = ArrayDeque<QueuedGeminiLiveEvent>()
    // Cancellation can discard a wakeup, but never a payload held by the deque.
    private val wakeup = Channel<Unit>(Channel.CONFLATED)

    val phase: GeminiLiveSessionPhase
        get() = state.get()

    val listener = object : GeminiLiveSocketListener {
        override fun onOpen() {
            synchronized(setupBudgetLock) {
                if (state.compareAndSet(GeminiLiveSessionPhase.OPENING, GeminiLiveSessionPhase.AWAITING_SETUP)) {
                    events.addLast(QueuedGeminiLiveEvent(GeminiLiveCoreEvent.Opened))
                    wakeup.trySend(Unit)
                }
            }
        }

        override fun onText(payload: String) = onPayload(payload, GeminiLiveWireFormat.TEXT)

        override fun onBinary(payload: ByteString) = onPayload(payload.utf8(), GeminiLiveWireFormat.BINARY)

        override fun onClosing(code: Int, reason: String) {
            terminate(GeminiLiveCoreEvent.Closed(code, reason), "Gemini Live peer closing")
        }

        override fun onClosed(code: Int, reason: String) {
            terminate(GeminiLiveCoreEvent.Closed(code, reason), "Gemini Live peer closed")
        }

        override fun onFailure(error: Throwable) {
            terminate(
                GeminiLiveCoreEvent.Failed(GeminiLiveSessionFailure.Transport(error)),
                "Gemini Live transport failed",
            )
        }
    }

    fun attach(connectedSocket: GeminiLiveSocket) {
        check(socket.compareAndSet(null, connectedSocket)) { "Gemini Live socket already attached" }
        shutdownSocketIfRequested()
    }

    fun activate(): Boolean = synchronized(setupBudgetLock) {
        state.compareAndSet(GeminiLiveSessionPhase.AWAITING_SETUP, GeminiLiveSessionPhase.ACTIVE)
    }

    fun sendSetup(payload: String): Boolean {
        if (state.get() != GeminiLiveSessionPhase.AWAITING_SETUP) {
            return false
        }
        return send(payload, GeminiLiveSessionFailure.SetupSendRejected)
    }

    fun sendActive(payload: String): Boolean {
        if (state.get() != GeminiLiveSessionPhase.ACTIVE) {
            return false
        }
        return send(payload, GeminiLiveSessionFailure.ActiveSendRejected)
    }

    private fun send(payload: String, rejection: GeminiLiveSessionFailure): Boolean {
        val connectedSocket = socket.get() ?: return false.also { fail(rejection) }
        val sent = runCatching { connectedSocket.send(payload) }.getOrElse { error ->
            fail(GeminiLiveSessionFailure.Transport(error))
            return false
        }
        if (!sent) {
            fail(rejection)
        }
        return sent
    }

    fun pollEvent(): GeminiLiveCoreEvent? = synchronized(setupBudgetLock) {
        val queued = events.pollFirst() ?: return@synchronized if (activeBufferOverflow) terminal.get() else null
            if (queued.payload) {
                queuedFrames--
                queuedBytes -= queued.payloadBytes
            }
            if (activeBufferOverflow) terminal.get() ?: queued.event else queued.event
    }

    suspend fun awaitEvent(): Boolean {
        while (true) {
            synchronized(setupBudgetLock) {
                if (events.isNotEmpty() || activeBufferOverflow) return true
                if (terminal.get() != null) return false
            }
            wakeup.receive()
        }
    }

    suspend fun receiveEvent(): GeminiLiveCoreEvent? {
        while (true) {
            pollEvent()?.let { return it }
            if (!awaitEvent()) return null
        }
    }

    fun <T> pollPending(take: () -> T?): Pair<GeminiLiveSessionFailure?, T?> = synchronized(setupBudgetLock) {
        if (activeBufferOverflow) {
            ((terminal.get() as? GeminiLiveCoreEvent.Failed)?.failure) to null
        } else {
            null to take()
        }
    }

    fun failureBeforeReady(): GeminiLiveSessionFailure? = when (val event = terminal.get()) {
        is GeminiLiveCoreEvent.Failed -> event.failure
        is GeminiLiveCoreEvent.Closed -> GeminiLiveSessionFailure.ClosedBeforeReady(event.code, event.reason)
        else -> null
    }

    fun terminalReceiveResult(): GeminiLiveReceiveResult = when (val event = terminal.get()) {
        is GeminiLiveCoreEvent.Closed -> GeminiLiveReceiveResult.Closed(event.code, event.reason)
        is GeminiLiveCoreEvent.Failed -> GeminiLiveReceiveResult.Failed(event.failure)
        else -> GeminiLiveReceiveResult.Closed(null, null)
    }

    fun fail(failure: GeminiLiveSessionFailure) {
        terminate(GeminiLiveCoreEvent.Failed(failure), "Gemini Live session failed")
    }

    fun cancel() {
        terminate(GeminiLiveCoreEvent.Closed(1000, "cancelled"), "Gemini Live session cancelled")
    }

    fun closeLocally() {
        terminate(GeminiLiveCoreEvent.Closed(1000, "closed locally"), "Gemini Live session closed")
    }

    private fun onPayload(payload: String, wireFormat: GeminiLiveWireFormat) {
        var shutdownReason: String? = null
        synchronized(setupBudgetLock) {
            if (terminal.get() != null) return
            val preReady = state.get() != GeminiLiveSessionPhase.ACTIVE
            val limit = if (preReady) MAX_SETUP_BYTES else MAX_ACTIVE_BYTES
            if (payload.length > limit) {
                val changed = if (preReady) markTerminated(setupOverflow()) else markActiveOverflow()
                if (changed) shutdownReason = "Gemini Live receive limit exceeded"
                return@synchronized
            }
            val payloadBytes = payload.toByteArray(Charsets.UTF_8).size
            if (payloadBytes > limit) {
                val changed = if (preReady) markTerminated(setupOverflow()) else markActiveOverflow()
                if (changed) shutdownReason = "Gemini Live receive limit exceeded"
                return@synchronized
            }
            val frame = parseFrame(payload)
            if (frame?.error != null) {
                if (markTerminated(GeminiLiveCoreEvent.Failed(GeminiLiveSessionFailure.Server(frame.error, frame.errorRetryable)))) {
                    shutdownReason = "Gemini Live server error"
                }
                return@synchronized
            }
            val retained = frame == null || !frame.setupComplete || frame.hasPostSetupObservation
            if (preReady && !retained) {
                if (setupAckQueued) return
            }
            if (preReady) {
                if (setupFrames >= MAX_SETUP_FRAMES || payloadBytes > MAX_SETUP_BYTES - setupBytes) {
                    if (markTerminated(setupOverflow())) shutdownReason = "Gemini Live receive limit exceeded"
                    return@synchronized
                } else {
                    setupFrames++
                    setupBytes += payloadBytes
                    if (!retained) setupAckQueued = true
                }
            }
            if (queuedFrames >= MAX_ACTIVE_FRAMES || payloadBytes > MAX_ACTIVE_BYTES - queuedBytes) {
                if (markActiveOverflow()) shutdownReason = "Gemini Live receive limit exceeded"
                return@synchronized
            }
            val event = if (frame == null) GeminiLiveCoreEvent.Unparsed(payload, wireFormat)
            else GeminiLiveCoreEvent.Frame(frame, wireFormat)
            queuedFrames++
            queuedBytes += payloadBytes
            events.addLast(QueuedGeminiLiveEvent(event, payloadBytes, payload = true))
            wakeup.trySend(Unit)
        }
        shutdownReason?.let(::shutdownSocketIfRequested)
    }

    private fun terminate(event: GeminiLiveCoreEvent, closeReason: String) {
        val changed = synchronized(setupBudgetLock) { markTerminated(event) }
        if (changed) shutdownSocketIfRequested(closeReason)
    }

    private fun markTerminated(event: GeminiLiveCoreEvent): Boolean {
        if (!terminal.compareAndSet(null, event)) {
            return false
        }
        state.getAndUpdate { current ->
            if (current == GeminiLiveSessionPhase.CLOSED) current else GeminiLiveSessionPhase.CLOSING
        }
        events.addLast(QueuedGeminiLiveEvent(event))
        wakeup.trySend(Unit)
        shutdownRequested.set(true)
        state.set(GeminiLiveSessionPhase.CLOSED)
        return true
    }

    private fun setupOverflow(): GeminiLiveCoreEvent = GeminiLiveCoreEvent.Failed(
        GeminiLiveSessionFailure.Server("Gemini Live setup observation limit exceeded."),
    )

    private fun activeOverflow(): GeminiLiveCoreEvent = GeminiLiveCoreEvent.Failed(
        GeminiLiveSessionFailure.Server("Gemini Live receive buffer limit exceeded; buffered events discarded."),
    )

    private fun markActiveOverflow(): Boolean {
        if (terminal.get() != null) return false
        activeBufferOverflow = true
        while (true) {
            val queued = events.pollFirst() ?: break
            if (queued.payload) {
                queuedFrames--
                queuedBytes -= queued.payloadBytes
            }
        }
        return markTerminated(activeOverflow())
    }


    private fun shutdownSocketIfRequested(reason: String = "Gemini Live session finished") {
        if (!shutdownRequested.get()) {
            return
        }
        val connectedSocket = socket.get() ?: return
        if (!socketShutdown.compareAndSet(false, true)) {
            return
        }
        runCatching { connectedSocket.close(1000, reason.take(100)) }
        runCatching { connectedSocket.cancel() }
    }
}

internal const val MAX_SETUP_FRAMES = 64
internal const val MAX_SETUP_BYTES = 1024 * 1024
internal const val MAX_ACTIVE_FRAMES = 128
internal const val MAX_ACTIVE_BYTES = 8 * 1024 * 1024
