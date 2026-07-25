package dev.screengoated.toolbox.mobile.phonecontrol.provider.browser

import java.io.Closeable
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.TimeoutCancellationException
import kotlinx.coroutines.withTimeout
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okio.ByteString

internal sealed interface CdpCommandResult {
    data class Success(val result: JsonObject) : CdpCommandResult

    data class Failure(
        val code: String,
        val message: String,
        val retryable: Boolean,
        val effectMayHaveOccurred: Boolean = false,
    ) : CdpCommandResult
}

internal class ChromeDevToolsSession private constructor(
    private val socket: WebSocket,
    private val opened: CompletableDeferred<Unit>,
    private val pending: ConcurrentHashMap<Int, CompletableDeferred<JsonObject>>,
    private val events: EventBuffer,
    private val closed: AtomicBoolean,
) : Closeable {
    private val nextId = AtomicInteger(1)

    suspend fun send(
        method: String,
        params: JsonObject = JsonObject(emptyMap()),
        timeoutMs: Long = COMMAND_TIMEOUT_MS,
    ): CdpCommandResult {
        if (!validMethod(method)) {
            return CdpCommandResult.Failure(
                "browser_cdp_invalid_method",
                "The DevTools method name is invalid.",
                retryable = false,
            )
        }
        try {
            withTimeout(OPEN_TIMEOUT_MS) { opened.await() }
        } catch (_: TimeoutCancellationException) {
            return CdpCommandResult.Failure(
                "browser_cdp_connect_timeout",
                "The DevTools target did not accept a WebSocket session.",
                retryable = true,
            )
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (_: CdpDisconnectedException) {
            return CdpCommandResult.Failure(
                "browser_cdp_disconnected",
                "The exact DevTools target disconnected before the request.",
                retryable = true,
            )
        }
        if (closed.get()) {
            return CdpCommandResult.Failure(
                "browser_cdp_disconnected",
                "The exact DevTools target session is closed.",
                retryable = true,
            )
        }
        val id = nextId.getAndIncrement()
        val response = CompletableDeferred<JsonObject>()
        pending[id] = response
        val payload = buildJsonObject {
            put("id", id)
            put("method", method)
            if (params.isNotEmpty()) put("params", params)
        }
        if (!socket.send(JSON.encodeToString(JsonObject.serializer(), payload))) {
            pending.remove(id)
            return CdpCommandResult.Failure(
                "browser_cdp_disconnected",
                "The DevTools request could not be queued.",
                retryable = true,
            )
        }
        return try {
            val message = withTimeout(timeoutMs.coerceIn(MIN_TIMEOUT_MS, MAX_TIMEOUT_MS)) {
                response.await()
            }
            val error = message["error"] as? JsonObject
            if (error != null) {
                CdpCommandResult.Failure(
                    code = "browser_cdp_command_rejected",
                    message = error["message"]?.jsonPrimitive?.contentOrNull
                        ?: "Chrome rejected the DevTools command.",
                    retryable = false,
                )
            } else {
                CdpCommandResult.Success(
                    (message["result"] as? JsonObject) ?: JsonObject(emptyMap()),
                )
            }
        } catch (_: TimeoutCancellationException) {
            CdpCommandResult.Failure(
                "browser_cdp_command_timeout",
                "The DevTools command timed out.",
                retryable = true,
                effectMayHaveOccurred = true,
            )
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (_: CdpDisconnectedException) {
            CdpCommandResult.Failure(
                "browser_cdp_disconnected",
                "The exact DevTools target disconnected after dispatch.",
                retryable = true,
                effectMayHaveOccurred = true,
            )
        } finally {
            pending.remove(id)
        }
    }

    fun networkEvents(filter: String?): List<JsonObject> =
        events.network(filter?.takeIf(String::isNotBlank))

    fun consoleEvents(): List<JsonObject> = events.console()

    override fun close() {
        if (!closed.compareAndSet(false, true)) return
        socket.close(NORMAL_CLOSE_CODE, "session retired")
        val error = CdpDisconnectedException()
        pending.values.forEach { it.completeExceptionally(error) }
        pending.clear()
        if (!opened.isCompleted) opened.completeExceptionally(error)
    }

    companion object {
        suspend fun open(
            transport: AndroidChromeDevToolsTransport,
            webSocketPath: String,
        ): BrowserTransportResult<ChromeDevToolsSession> {
            val opened = CompletableDeferred<Unit>()
            val pending = ConcurrentHashMap<Int, CompletableDeferred<JsonObject>>()
            val events = EventBuffer()
            val closed = AtomicBoolean(false)
            val listener = object : WebSocketListener() {
                override fun onOpen(webSocket: WebSocket, response: Response) {
                    opened.complete(Unit)
                }

                override fun onMessage(webSocket: WebSocket, text: String) {
                    receive(text, pending, events)
                }

                override fun onMessage(webSocket: WebSocket, bytes: ByteString) {
                    receive(bytes.utf8(), pending, events)
                }

                override fun onClosing(
                    webSocket: WebSocket,
                    code: Int,
                    reason: String,
                ) {
                    webSocket.close(code, reason)
                }

                override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                    closePending(opened, pending, closed)
                }

                override fun onFailure(
                    webSocket: WebSocket,
                    t: Throwable,
                    response: Response?,
                ) {
                    closePending(opened, pending, closed)
                }
            }
            return when (val socket = transport.openWebSocket(webSocketPath, listener)) {
                is BrowserTransportResult.Ready -> {
                    val session = ChromeDevToolsSession(
                        socket.value,
                        opened,
                        pending,
                        events,
                        closed,
                    )
                    try {
                        withTimeout(OPEN_TIMEOUT_MS) { opened.await() }
                        BrowserTransportResult.Ready(session)
                    } catch (_: TimeoutCancellationException) {
                        session.close()
                        BrowserTransportResult.Failure(
                            dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState.DEGRADED,
                            "browser_cdp_connect_timeout",
                            "The exact DevTools target did not accept a WebSocket session.",
                        )
                    } catch (cancelled: CancellationException) {
                        session.close()
                        throw cancelled
                    } catch (_: CdpDisconnectedException) {
                        session.close()
                        BrowserTransportResult.Failure(
                            dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState.DEGRADED,
                            "browser_cdp_disconnected",
                            "The exact DevTools target disconnected during setup.",
                        )
                    }
                }
                is BrowserTransportResult.Failure -> socket
            }
        }

        private fun receive(
            raw: String,
            pending: ConcurrentHashMap<Int, CompletableDeferred<JsonObject>>,
            events: EventBuffer,
        ) {
            val message = runCatching { JSON.parseToJsonElement(raw).jsonObject }.getOrNull()
                ?: return
            val id = message["id"]?.jsonPrimitive?.intOrNull
            if (id != null) {
                pending[id]?.complete(message)
                return
            }
            val method = message["method"]?.jsonPrimitive?.contentOrNull ?: return
            events.add(method, message)
        }

        private fun closePending(
            opened: CompletableDeferred<Unit>,
            pending: ConcurrentHashMap<Int, CompletableDeferred<JsonObject>>,
            closed: AtomicBoolean,
        ) {
            if (!closed.compareAndSet(false, true)) return
            val error = CdpDisconnectedException()
            if (!opened.isCompleted) opened.completeExceptionally(error)
            pending.values.forEach { it.completeExceptionally(error) }
            pending.clear()
        }

        private val JSON = Json { ignoreUnknownKeys = true }
        private const val OPEN_TIMEOUT_MS = 8_000L
        private const val COMMAND_TIMEOUT_MS = 12_000L
        private const val MIN_TIMEOUT_MS = 100L
        private const val MAX_TIMEOUT_MS = 30_000L
        private const val NORMAL_CLOSE_CODE = 1_000

        private fun validMethod(value: String): Boolean =
            value.isNotBlank() &&
                value.length <= 128 &&
                value.all { it.isLetterOrDigit() || it == '.' || it == '_' }
    }
}

private class CdpDisconnectedException : RuntimeException("DevTools target disconnected")

private class EventBuffer {
    private val lock = Any()
    private val network = ArrayDeque<JsonObject>()
    private val console = ArrayDeque<JsonObject>()

    fun add(method: String, message: JsonObject) {
        synchronized(lock) {
            when {
                method.startsWith("Network.") -> network.addBounded(message)
                method.startsWith("Runtime.") || method.startsWith("Log.") ->
                    console.addBounded(message)
            }
        }
    }

    fun network(filter: String?): List<JsonObject> = synchronized(lock) {
        network.filter { event ->
            filter == null ||
                event["method"]?.jsonPrimitive?.contentOrNull?.contains(filter, ignoreCase = true) == true
        }.takeLast(MAX_RETURNED_EVENTS)
    }

    fun console(): List<JsonObject> = synchronized(lock) {
        console.takeLast(MAX_RETURNED_EVENTS)
    }

    private fun ArrayDeque<JsonObject>.addBounded(value: JsonObject) {
        addLast(value)
        while (size > MAX_BUFFERED_EVENTS) removeFirst()
    }

    private companion object {
        const val MAX_BUFFERED_EVENTS = 200
        const val MAX_RETURNED_EVENTS = 100
    }
}
