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
                    listener.onFailure(t)
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

internal class GeminiLiveSessionCore {
    private val state = AtomicReference(GeminiLiveSessionPhase.OPENING)
    private val socket = AtomicReference<GeminiLiveSocket?>(null)
    private val terminal = AtomicReference<GeminiLiveCoreEvent?>(null)
    private val shutdownRequested = AtomicBoolean(false)
    private val socketShutdown = AtomicBoolean(false)
    private val events = Channel<GeminiLiveCoreEvent>(Channel.UNLIMITED)

    val phase: GeminiLiveSessionPhase
        get() = state.get()

    val listener = object : GeminiLiveSocketListener {
        override fun onOpen() {
            if (state.compareAndSet(GeminiLiveSessionPhase.OPENING, GeminiLiveSessionPhase.AWAITING_SETUP)) {
                events.trySend(GeminiLiveCoreEvent.Opened)
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

    fun activate(): Boolean =
        state.compareAndSet(GeminiLiveSessionPhase.AWAITING_SETUP, GeminiLiveSessionPhase.ACTIVE)

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

    suspend fun receiveEvent(): GeminiLiveCoreEvent? = events.receiveCatching().getOrNull()

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
        if (terminal.get() != null) {
            return
        }
        val frame = parseGeminiLiveServerFrame(payload)
        if (frame == null) {
            events.trySend(GeminiLiveCoreEvent.Unparsed(payload, wireFormat))
            return
        }
        frame.error?.let { error ->
            terminate(
                GeminiLiveCoreEvent.Failed(
                    GeminiLiveSessionFailure.Server(error, retryable = frame.errorRetryable),
                ),
                "Gemini Live server error",
            )
            return
        }
        events.trySend(GeminiLiveCoreEvent.Frame(frame, wireFormat))
    }

    private fun terminate(event: GeminiLiveCoreEvent, closeReason: String) {
        if (!terminal.compareAndSet(null, event)) {
            return
        }
        state.getAndUpdate { current ->
            if (current == GeminiLiveSessionPhase.CLOSED) current else GeminiLiveSessionPhase.CLOSING
        }
        events.trySend(event)
        events.close()
        requestShutdown(closeReason)
        state.set(GeminiLiveSessionPhase.CLOSED)
    }

    private fun requestShutdown(reason: String) {
        shutdownRequested.set(true)
        shutdownSocketIfRequested(reason)
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
