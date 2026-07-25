package dev.screengoated.toolbox.mobile.phonecontrol.provider.browser

import android.content.Context
import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import java.io.Closeable
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import okhttp3.HttpUrl
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okhttp3.RequestBody.Companion.toRequestBody

internal sealed interface BrowserTransportResult<out T> {
    data class Ready<T>(val value: T) : BrowserTransportResult<T>

    data class Failure(
        val state: CapabilityState,
        val code: String,
        val message: String,
        val requiredUserStep: String? = null,
        val retryable: Boolean = true,
        val effectMayHaveOccurred: Boolean = false,
    ) : BrowserTransportResult<Nothing>
}

internal class AndroidChromeDevToolsTransport(
    context: Context,
) : Closeable {
    private val appContext = context.applicationContext
    private val endpointMutex = Mutex()
    private val json = Json { ignoreUnknownKeys = true }
    private val client = OkHttpClient.Builder()
        .connectTimeout(CONNECT_TIMEOUT_MS, TimeUnit.MILLISECONDS)
        .readTimeout(REQUEST_TIMEOUT_MS, TimeUnit.MILLISECONDS)
        .writeTimeout(REQUEST_TIMEOUT_MS, TimeUnit.MILLISECONDS)
        .callTimeout(REQUEST_TIMEOUT_MS, TimeUnit.MILLISECONDS)
        .build()

    @Volatile
    private var endpoint: Endpoint? = null
    private val closed = AtomicBoolean(false)

    val authorityProviderId: String?
        get() = AndroidChromeCdpAuthority.probe(appContext).providerId

    suspend fun probe(): BrowserTransportResult<JsonElement> =
        requestJson(method = "GET", path = "/json/version")

    suspend fun requestJson(
        method: String,
        path: String,
        query: Map<String, String> = emptyMap(),
    ): BrowserTransportResult<JsonElement> {
        if (method !in SUPPORTED_METHODS || !validPath(path)) {
            return BrowserTransportResult.Failure(
                CapabilityState.UNSUPPORTED,
                "browser_transport_invalid_request",
                "The browser transport request is invalid.",
                retryable = false,
            )
        }
        repeat(2) { attempt ->
            val active = when (val opened = ensureEndpoint()) {
                is BrowserTransportResult.Ready -> opened.value
                is BrowserTransportResult.Failure -> return opened
            }
            val response = execute(active, method, path, query)
            if (
                response !is BrowserTransportResult.Failure ||
                !response.shouldRenewTunnel() ||
                response.effectMayHaveOccurred
            ) {
                return response
            }
            if (attempt == 0) retireEndpoint(active)
        }
        return BrowserTransportResult.Failure(
            CapabilityState.DEGRADED,
            "browser_transport_unavailable",
            "The authenticated browser tunnel could not be renewed.",
            requiredUserStep = "repair_selected_browser_authority",
        )
    }

    suspend fun requestStatus(
        method: String,
        path: String,
        query: Map<String, String> = emptyMap(),
    ): BrowserTransportResult<Unit> {
        if (method !in SUPPORTED_METHODS || !validPath(path)) {
            return BrowserTransportResult.Failure(
                CapabilityState.UNSUPPORTED,
                "browser_transport_invalid_request",
                "The browser transport request is invalid.",
                retryable = false,
            )
        }
        repeat(2) { attempt ->
            val active = when (val opened = ensureEndpoint()) {
                is BrowserTransportResult.Ready -> opened.value
                is BrowserTransportResult.Failure -> return opened
            }
            val response = executeStatus(active, method, path, query)
            if (
                response !is BrowserTransportResult.Failure ||
                !response.shouldRenewTunnel() ||
                response.effectMayHaveOccurred
            ) {
                return response
            }
            if (attempt == 0) retireEndpoint(active)
        }
        return BrowserTransportResult.Failure(
            CapabilityState.DEGRADED,
            "browser_transport_unavailable",
            "The authenticated browser tunnel could not be renewed.",
            requiredUserStep = "repair_selected_browser_authority",
        )
    }

    suspend fun openWebSocket(
        path: String,
        listener: WebSocketListener,
    ): BrowserTransportResult<WebSocket> {
        if (!validDevToolsWebSocketPath(path)) {
            return BrowserTransportResult.Failure(
                CapabilityState.UNSUPPORTED,
                "browser_target_invalid",
                "Chrome returned an invalid DevTools target path.",
                retryable = false,
            )
        }
        val active = when (val opened = ensureEndpoint()) {
            is BrowserTransportResult.Ready -> opened.value
            is BrowserTransportResult.Failure -> return opened
        }
        val request = Request.Builder()
            .url(active.url(path))
            .header(AUTH_HEADER, active.bearerToken)
            .build()
        return BrowserTransportResult.Ready(client.newWebSocket(request, listener))
    }

    override fun close() {
        cleanupScope.launch { shutdown() }
    }

    suspend fun shutdown() {
        if (!closed.compareAndSet(false, true)) return
        val retired = endpointMutex.withLock {
            endpoint.also { endpoint = null }
        }
        client.dispatcher.cancelAll()
        client.dispatcher.executorService.shutdown()
        client.connectionPool.evictAll()
        retired?.let {
            withTimeoutOrNull(TUNNEL_CLOSE_TIMEOUT_MS) {
                AndroidChromeCdpAuthority.close(
                    appContext,
                    it.authorityProviderId,
                    it.leaseId,
                )
            }
        }
    }

    private suspend fun ensureEndpoint(): BrowserTransportResult<Endpoint> =
        endpointMutex.withLock {
            if (closed.get()) {
                return@withLock BrowserTransportResult.Failure(
                    CapabilityState.UNAVAILABLE,
                    "browser_transport_closed",
                    "The browser transport is closed.",
                    retryable = false,
                )
            }
            val selected = AndroidChromeCdpAuthority.probe(appContext).providerId
            endpoint?.let { current ->
                if (current.authorityProviderId == selected) {
                    return@withLock BrowserTransportResult.Ready(current)
                }
                endpoint = null
                AndroidChromeCdpAuthority.close(
                    appContext,
                    current.authorityProviderId,
                    current.leaseId,
                )
            }
            when (val opened = AndroidChromeCdpAuthority.open(appContext)) {
                is ChromeCdpTunnelOpenResult.Ready -> {
                    val value = Endpoint(
                        authorityProviderId = opened.providerId,
                        leaseId = opened.lease.leaseId,
                        port = opened.lease.port,
                        bearerToken = opened.lease.bearerToken,
                    )
                    endpoint = value
                    BrowserTransportResult.Ready(value)
                }
                is ChromeCdpTunnelOpenResult.Failure -> BrowserTransportResult.Failure(
                    state = opened.result.state,
                    code = opened.result.code,
                    message = "The authenticated local browser tunnel is unavailable.",
                    requiredUserStep = opened.result.requiredUserStep,
                )
            }
        }

    private suspend fun execute(
        active: Endpoint,
        method: String,
        path: String,
        query: Map<String, String>,
    ): BrowserTransportResult<JsonElement> = withContext(Dispatchers.IO) {
        val url = active.url(path, query)
        val request = Request.Builder()
            .url(url)
            .header(AUTH_HEADER, active.bearerToken)
            .method(
                method,
                if (method == "POST" || method == "PUT") {
                    ByteArray(0).toRequestBody()
                } else {
                    null
                },
            )
            .build()
        try {
            client.newCall(request).execute().use { response ->
                val body = response.body.string()
                if (!response.isSuccessful) {
                    return@withContext httpFailure(response.code)
                }
                val parsed = runCatching { json.parseToJsonElement(body) }.getOrElse {
                    return@withContext BrowserTransportResult.Failure(
                        CapabilityState.DEGRADED,
                        "browser_transport_invalid_response",
                        "Chrome returned an invalid DevTools response.",
                        effectMayHaveOccurred = method != "GET",
                    )
                }
                BrowserTransportResult.Ready(parsed)
            }
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (_: Throwable) {
            BrowserTransportResult.Failure(
                CapabilityState.DEGRADED,
                "browser_transport_io_failed",
                "The authenticated DevTools transport disconnected.",
                effectMayHaveOccurred = method != "GET",
            )
        }
    }

    private suspend fun executeStatus(
        active: Endpoint,
        method: String,
        path: String,
        query: Map<String, String>,
    ): BrowserTransportResult<Unit> = withContext(Dispatchers.IO) {
        val effectMayHaveOccurred = path.startsWith("/json/activate/") ||
            path.startsWith("/json/close/")
        val request = Request.Builder()
            .url(active.url(path, query))
            .header(AUTH_HEADER, active.bearerToken)
            .method(
                method,
                if (method == "POST" || method == "PUT") {
                    ByteArray(0).toRequestBody()
                } else {
                    null
                },
            )
            .build()
        try {
            client.newCall(request).execute().use { response ->
                if (response.isSuccessful) {
                    BrowserTransportResult.Ready(Unit)
                } else {
                    httpFailure(response.code)
                }
            }
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (_: Throwable) {
            BrowserTransportResult.Failure(
                CapabilityState.DEGRADED,
                "browser_transport_io_failed",
                "The authenticated DevTools transport disconnected.",
                effectMayHaveOccurred = effectMayHaveOccurred,
            )
        }
    }

    private suspend fun retireEndpoint(expected: Endpoint) {
        endpointMutex.withLock {
            if (endpoint !== expected) return
            endpoint = null
            AndroidChromeCdpAuthority.close(
                appContext,
                expected.authorityProviderId,
                expected.leaseId,
            )
        }
    }

    private fun httpFailure(status: Int): BrowserTransportResult.Failure = when (status) {
        403 -> BrowserTransportResult.Failure(
            CapabilityState.REVOKED,
            "browser_tunnel_auth_rejected",
            "The browser tunnel lease was rejected.",
            requiredUserStep = "repair_selected_browser_authority",
        )
        502 -> BrowserTransportResult.Failure(
            CapabilityState.UNAVAILABLE,
            "browser_endpoint_unavailable",
            "No compatible running browser exposes a DevTools endpoint.",
            requiredUserStep = "open_supported_browser",
        )
        503 -> BrowserTransportResult.Failure(
            CapabilityState.DEGRADED,
            "browser_tunnel_busy",
            "The browser tunnel is at its bounded connection limit.",
        )
        else -> BrowserTransportResult.Failure(
            CapabilityState.DEGRADED,
            "browser_devtools_http_$status",
            "Chrome rejected the DevTools request.",
        )
    }

    private data class Endpoint(
        val authorityProviderId: String,
        val leaseId: String,
        val port: Int,
        val bearerToken: String,
    ) {
        fun url(path: String, query: Map<String, String> = emptyMap()): HttpUrl {
            val builder = HttpUrl.Builder()
                .scheme("http")
                .host(LOOPBACK_ADDRESS)
                .port(port)
            path.trimStart('/').split('/').filter(String::isNotEmpty).forEach {
                builder.addPathSegment(it)
            }
            query.forEach { (name, value) -> builder.addQueryParameter(name, value) }
            return builder.build()
        }
    }

    private fun BrowserTransportResult.Failure.shouldRenewTunnel(): Boolean =
        code in setOf("browser_tunnel_auth_rejected", "browser_transport_io_failed")

    private companion object {
        const val LOOPBACK_ADDRESS = "127.0.0.1"
        const val AUTH_HEADER = "X-SGT-Bridge-Token"
        const val CONNECT_TIMEOUT_MS = 5_000L
        const val REQUEST_TIMEOUT_MS = 12_000L
        const val TUNNEL_CLOSE_TIMEOUT_MS = 5_000L
        val SUPPORTED_METHODS = setOf("GET", "PUT", "POST", "DELETE")
        val cleanupScope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

        fun validPath(path: String): Boolean =
            path.startsWith('/') &&
                !path.contains('\\') &&
                !path.contains('\u0000') &&
                !path.contains('?') &&
                !path.contains('#')

        fun validDevToolsWebSocketPath(path: String): Boolean =
            validPath(path) && path.startsWith("/devtools/")
    }
}
