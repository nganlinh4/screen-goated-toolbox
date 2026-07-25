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
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.Executors
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
        clients.toList().forEach(Client::close)
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
            if (!slots.tryAcquire()) {
                socket.reject(HTTP_UNAVAILABLE)
                continue
            }
            executor.execute {
                try {
                    serve(socket)
                } finally {
                    slots.release()
                }
            }
        }
    }

    private fun serve(socket: Socket) {
        socket.tcpNoDelay = true
        socket.soTimeout = HEADER_TIMEOUT_MS
        val request = runCatching { readAuthenticatedRequest(socket) }.getOrNull()
        if (request == null) {
            socket.reject(HTTP_FORBIDDEN)
            return
        }
        val upstream = try {
            openUpstream()
        } catch (_: Throwable) {
            socket.reject(HTTP_BAD_GATEWAY)
            return
        }
        val client = Client(socket, upstream)
        clients += client
        try {
            socket.soTimeout = 0
            upstream.output.write(request)
            upstream.output.flush()
            val uplink = Thread(
                {
                    runCatching {
                        socket.getInputStream().copyTo(upstream.output, COPY_BUFFER_BYTES)
                        upstream.output.flush()
                    }
                    client.close()
                },
                "sgt-cdp-upstream",
            ).apply {
                isDaemon = true
                start()
            }
            runCatching {
                upstream.input.copyTo(socket.getOutputStream(), COPY_BUFFER_BYTES)
                socket.getOutputStream().flush()
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

    private fun readAuthenticatedRequest(socket: Socket): ByteArray? {
        val input = socket.getInputStream()
        val header = ByteArrayOutputStream()
        var matched = 0
        while (header.size() <= MAX_HEADER_BYTES) {
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
        return sanitizedAuthenticatedHeader(header.toByteArray(), lease.bearerToken)
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
        private const val MAX_HEADER_BYTES = 32 * 1_024
        private const val HEADER_TIMEOUT_MS = 5_000
        private const val COPY_BUFFER_BYTES = 16 * 1_024
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
