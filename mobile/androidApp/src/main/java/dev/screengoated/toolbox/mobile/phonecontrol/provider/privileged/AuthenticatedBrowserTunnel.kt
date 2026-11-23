package dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged

import java.io.ByteArrayOutputStream
import java.io.Closeable
import java.io.InputStream
import java.io.OutputStream
import java.net.InetAddress
import java.net.ServerSocket
import java.net.Socket
import java.nio.charset.StandardCharsets
import java.security.MessageDigest
import java.security.SecureRandom
import java.util.Base64
import java.util.Deque
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.ConcurrentLinkedDeque
import java.util.concurrent.Executors
import java.util.concurrent.RejectedExecutionException
import java.util.concurrent.Semaphore
import java.util.concurrent.ThreadFactory
import java.util.concurrent.atomic.AtomicBoolean

internal data class BrowserTunnelLease(
    val leaseId: String,
    val port: Int,
    val bearerToken: String,
)

internal interface BrowserTunnelUpstream : Closeable {
    val input: InputStream
    val output: OutputStream
}

/**
 * Bounded authenticated loopback proxy. The provider-specific process supplies
 * one fresh device-local upstream per accepted client; this class owns no ADB,
 * Shizuku, root, browser, or app-specific routing decisions.
 */
internal class AuthenticatedBrowserTunnel(
    private val openUpstream: () -> BrowserTunnelUpstream,
) : Closeable {
    private val running = AtomicBoolean(true)
    private val slots = Semaphore(MAX_CLIENTS)
    private val handshakes = ConcurrentLinkedDeque<Socket>()
    private val clients = ConcurrentHashMap.newKeySet<Client>()
    private val executor = Executors.newCachedThreadPool(TunnelThreadFactory())
    private val server = ServerSocket(
        0,
        LISTEN_BACKLOG,
        InetAddress.getByName(LOOPBACK_ADDRESS),
    )
    val lease = BrowserTunnelLease(
        leaseId = UUID.randomUUID().toString(),
        port = server.localPort,
        bearerToken = newBearerToken(),
    )

    init {
        executor.execute(::acceptLoop)
    }

    val isOpen: Boolean
        get() = running.get() && !server.isClosed

    override fun close() {
        if (!running.compareAndSet(true, false)) return
        runCatching(server::close)
        closeDrained(handshakes)
        // toArray snapshots safely while serving threads remove clients.
        clients.toTypedArray().forEach(Client::close)
        executor.shutdownNow()
    }

    private fun acceptLoop() {
        while (running.get()) {
            val socket = try {
                server.accept()
            } catch (_: Throwable) {
                if (!running.get()) return
                continue
            }
            admitHandshake(socket)
            try {
                executor.execute { serve(socket) }
            } catch (_: RejectedExecutionException) {
                handshakes.remove(socket)
                runCatching(socket::close)
                return
            }
        }
    }

    /**
     * Unauthenticated sockets never hold a client slot. They wait in a bounded
     * handshake queue where the oldest is evicted first, so slow local
     * connections cannot lock out a client that presents its token promptly.
     */
    private fun admitHandshake(socket: Socket) {
        handshakes.addLast(socket)
        while (handshakes.size > MAX_PENDING_HANDSHAKES) {
            handshakes.pollFirst()?.let { runCatching(it::close) }
        }
    }

    private fun serve(socket: Socket) {
        val request = try {
            runCatching {
                socket.tcpNoDelay = true
                readAuthenticatedRequest(socket)
            }.getOrNull()
        } finally {
            handshakes.remove(socket)
        }
        if (request == null) {
            socket.reject(HTTP_FORBIDDEN)
            return
        }
        if (!slots.tryAcquire()) {
            socket.reject(HTTP_UNAVAILABLE)
            return
        }
        try {
            runCatching { relay(socket, request) }.onFailure { runCatching(socket::close) }
        } finally {
            slots.release()
        }
    }

    /**
     * Only the first request's header is authenticated and stripped, so a
     * connection carries exactly that request, or the WebSocket it upgrades to.
     */
    private fun relay(socket: Socket, request: ForwardedTunnelRequest) {
        val upstream = try {
            openUpstream()
        } catch (_: Throwable) {
            socket.reject(HTTP_BAD_GATEWAY)
            return
        }
        val client = Client(socket, upstream)
        clients += client
        val upgraded = AtomicBoolean(false)
        try {
            socket.soTimeout = 0
            upstream.output.write(request.header)
            upstream.output.flush()
            val uplink = Thread(
                {
                    runCatching {
                        forwardTunnelClientStream(
                            socket.getInputStream(),
                            upstream.output,
                            request.bodyLength,
                            upgraded,
                        )
                    }
                    client.close()
                },
                "sgt-cdp-upstream",
            ).apply {
                isDaemon = true
                start()
            }
            runCatching {
                val output = socket.getOutputStream()
                val statusLine = readTunnelStatusLine(upstream.input)
                if (isSwitchingProtocols(statusLine)) upgraded.set(true)
                output.write(statusLine)
                output.flush()
                copyTunnelStreamFlushing(upstream.input, output)
            }
            client.close()
            try {
                uplink.join(COPY_SETTLE_MS)
            } catch (_: InterruptedException) {
                Thread.currentThread().interrupt()
            }
        } finally {
            client.close()
            clients -= client
        }
    }

    private fun readAuthenticatedRequest(socket: Socket): ForwardedTunnelRequest? {
        val input = socket.getInputStream()
        val header = ByteArrayOutputStream()
        val deadlineNanos = System.nanoTime() + HEADER_TIMEOUT_MS * NANOS_PER_MILLI
        var matched = 0
        while (header.size() <= MAX_HEADER_BYTES) {
            socket.soTimeout = headerReadTimeoutMs(deadlineNanos, System.nanoTime()) ?: return null
            val next = input.read()
            if (next < 0) return null
            header.write(next)
            matched = if (next == HEADER_TERMINATOR[matched].toInt()) {
                matched + 1
            } else if (next == HEADER_TERMINATOR[0].toInt()) {
                1
            } else {
                0
            }
            if (matched == HEADER_TERMINATOR.size) break
        }
        if (matched != HEADER_TERMINATOR.size) return null
        val sanitized = sanitizedAuthenticatedHeader(header.toByteArray(), lease.bearerToken) ?: return null
        val bodyLength = tunnelRequestBodyLength(sanitized) ?: return null
        return ForwardedTunnelRequest(sanitized, bodyLength)
    }

    private class Client(
        private val socket: Socket,
        private val upstream: BrowserTunnelUpstream,
    ) : Closeable {
        private val closed = AtomicBoolean(false)

        override fun close() {
            if (!closed.compareAndSet(false, true)) return
            runCatching(upstream::close)
            runCatching(socket::close)
        }
    }

    private class TunnelThreadFactory : ThreadFactory {
        override fun newThread(task: Runnable): Thread =
            Thread(task, "sgt-cdp-tunnel").apply { isDaemon = true }
    }

    private fun Socket.reject(response: ByteArray) {
        runCatching {
            soTimeout = HEADER_TIMEOUT_MS
            getOutputStream().write(response)
            getOutputStream().flush()
        }
        runCatching(::close)
    }

    internal companion object {
        const val LOOPBACK_ADDRESS = "127.0.0.1"
        const val AUTH_HEADER = "X-SGT-Bridge-Token"
        private const val LISTEN_BACKLOG = 16
        private const val MAX_CLIENTS = 16
        const val MAX_PENDING_HANDSHAKES = 16
        private const val NANOS_PER_MILLI = 1_000_000L
        private const val MAX_HEADER_BYTES = 32 * 1_024
        private const val HEADER_TIMEOUT_MS = 5_000
        private const val COPY_SETTLE_MS = 500L
        private val SUPPORTED_METHODS = setOf("GET", "PUT", "POST", "DELETE", "OPTIONS")
        private val HEADER_TERMINATOR =
            "\r\n\r\n".toByteArray(StandardCharsets.ISO_8859_1)
        private val HTTP_FORBIDDEN =
            "HTTP/1.1 403 Forbidden\r\nConnection: close\r\nContent-Length: 0\r\n\r\n"
                .toByteArray(StandardCharsets.ISO_8859_1)
        private val HTTP_BAD_GATEWAY =
            "HTTP/1.1 502 Bad Gateway\r\nConnection: close\r\nContent-Length: 0\r\n\r\n"
                .toByteArray(StandardCharsets.ISO_8859_1)
        private val HTTP_UNAVAILABLE =
            "HTTP/1.1 503 Service Unavailable\r\nConnection: close\r\nContent-Length: 0\r\n\r\n"
                .toByteArray(StandardCharsets.ISO_8859_1)

        fun sanitizedAuthenticatedHeader(
            header: ByteArray,
            bearerToken: String,
        ): ByteArray? {
            val text = header.toString(StandardCharsets.ISO_8859_1)
            if (!text.endsWith("\r\n\r\n")) return null
            val lines = text.removeSuffix("\r\n\r\n").split("\r\n")
            if (lines.isEmpty() || !validRequestLine(lines.first())) return null
            val tokenLines = lines.drop(1).filter { line ->
                line.substringBefore(':', "").trim().equals(AUTH_HEADER, ignoreCase = true)
            }
            if (tokenLines.size != 1) return null
            val supplied = tokenLines.single().substringAfter(':', "").trim()
            if (!constantTimeEquals(supplied, bearerToken)) return null
            return buildString {
                append(lines.first()).append("\r\n")
                lines.drop(1).forEach { line ->
                    if (!line.substringBefore(':', "").trim().equals(AUTH_HEADER, true)) {
                        append(line).append("\r\n")
                    }
                }
                append("\r\n")
            }.toByteArray(StandardCharsets.ISO_8859_1)
        }

        /** Remaining whole-header budget as a socket read timeout; null once spent. */
        fun headerReadTimeoutMs(deadlineNanos: Long, nowNanos: Long): Int? {
            val remainingNanos = deadlineNanos - nowNanos
            if (remainingNanos <= 0L) return null
            return ((remainingNanos + NANOS_PER_MILLI - 1) / NANOS_PER_MILLI)
                .coerceIn(1L, HEADER_TIMEOUT_MS.toLong())
                .toInt()
        }

        fun newBearerToken(): String {
            val bytes = ByteArray(32)
            SecureRandom().nextBytes(bytes)
            return Base64.getUrlEncoder().withoutPadding().encodeToString(bytes)
        }

        fun constantTimeEquals(left: String, right: String): Boolean =
            MessageDigest.isEqual(
                left.toByteArray(StandardCharsets.UTF_8),
                right.toByteArray(StandardCharsets.UTF_8),
            )

        private fun validRequestLine(line: String): Boolean {
            val parts = line.split(' ')
            return parts.size == 3 &&
                parts[0] in SUPPORTED_METHODS &&
                parts[1].startsWith('/') &&
                parts[2].startsWith("HTTP/1.")
        }
    }
}

/**
 * Closes every queued socket while serving threads remove entries
 * concurrently. Kotlin's toList() reads size before iterating, so it can throw
 * NoSuchElementException on a shrinking concurrent deque; polling cannot.
 */
internal fun closeDrained(sockets: Deque<Socket>) {
    while (true) {
        val socket = sockets.pollFirst() ?: break
        runCatching(socket::close)
    }
}
