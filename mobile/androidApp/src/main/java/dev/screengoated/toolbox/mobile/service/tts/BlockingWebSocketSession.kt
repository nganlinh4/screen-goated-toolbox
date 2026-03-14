package dev.screengoated.toolbox.mobile.service.tts

import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okio.ByteString
import java.io.Closeable
import java.util.concurrent.CountDownLatch
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean

internal sealed interface WebSocketEvent {
    data class Text(
        val payload: String,
    ) : WebSocketEvent

    data class Binary(
        val payload: ByteString,
    ) : WebSocketEvent

    data class Failure(
        val throwable: Throwable,
    ) : WebSocketEvent

    data object Closed : WebSocketEvent
}

internal class BlockingWebSocketSession(
    client: OkHttpClient,
    request: Request,
) : Closeable {
    private val openSignal = CountDownLatch(1)
    private val openedSuccessfully = AtomicBoolean(false)
    private val events = LinkedBlockingQueue<WebSocketEvent>()
    private val socket: WebSocket

    init {
        socket = client.newWebSocket(
            request,
            object : WebSocketListener() {
                override fun onOpen(
                    webSocket: WebSocket,
                    response: Response,
                ) {
                    openedSuccessfully.set(true)
                    openSignal.countDown()
                }

                override fun onMessage(
                    webSocket: WebSocket,
                    text: String,
                ) {
                    events.offer(WebSocketEvent.Text(text))
                }

                override fun onMessage(
                    webSocket: WebSocket,
                    bytes: ByteString,
                ) {
                    events.offer(WebSocketEvent.Binary(bytes))
                }

                override fun onClosing(
                    webSocket: WebSocket,
                    code: Int,
                    reason: String,
                ) {
                    events.offer(WebSocketEvent.Closed)
                }

                override fun onClosed(
                    webSocket: WebSocket,
                    code: Int,
                    reason: String,
                ) {
                    events.offer(WebSocketEvent.Closed)
                }

                override fun onFailure(
                    webSocket: WebSocket,
                    t: Throwable,
                    response: Response?,
                ) {
                    openSignal.countDown()
                    events.offer(WebSocketEvent.Failure(t))
                }
            },
        )
    }

    fun awaitOpen(timeoutMs: Long): Boolean {
        if (!openSignal.await(timeoutMs, TimeUnit.MILLISECONDS)) {
            return false
        }
        return openedSuccessfully.get()
    }

    fun sendText(payload: String): Boolean = socket.send(payload)

    fun poll(timeoutMs: Long): WebSocketEvent? = events.poll(timeoutMs, TimeUnit.MILLISECONDS)

    override fun close() {
        socket.close(1000, "SGT TTS done")
        socket.cancel()
    }
}
