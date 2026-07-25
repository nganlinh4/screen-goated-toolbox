package dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged

import androidx.test.ext.junit.runners.AndroidJUnit4
import java.net.InetAddress
import java.net.ServerSocket
import java.net.Socket
import java.nio.charset.StandardCharsets
import java.util.concurrent.CompletableFuture
import java.util.concurrent.TimeUnit
import kotlin.concurrent.thread
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class AuthenticatedBrowserTunnelDeviceTest {
    @Test
    fun authenticatedLoopbackLeaseProxiesAndStripsItsSecret() {
        val upstreamServer = ServerSocket(
            0,
            1,
            InetAddress.getByName(AuthenticatedBrowserTunnel.LOOPBACK_ADDRESS),
        )
        val received = CompletableFuture<String>()
        val upstreamThread = thread(name = "fake-cdp-upstream", isDaemon = true) {
            upstreamServer.accept().use { socket ->
                val request = readHeader(socket)
                received.complete(request)
                socket.getOutputStream().write(
                    "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n[]"
                        .toByteArray(StandardCharsets.ISO_8859_1),
                )
                socket.getOutputStream().flush()
            }
        }
        val tunnel = AuthenticatedBrowserTunnel {
            SocketBrowserTunnelUpstream(
                Socket(
                    AuthenticatedBrowserTunnel.LOOPBACK_ADDRESS,
                    upstreamServer.localPort,
                ),
            )
        }
        try {
            val response = Socket(
                AuthenticatedBrowserTunnel.LOOPBACK_ADDRESS,
                tunnel.lease.port,
            ).use { client ->
                client.soTimeout = IO_TIMEOUT_MS
                client.getOutputStream().write(
                    (
                        "GET /json/list HTTP/1.1\r\n" +
                            "Host: 127.0.0.1\r\n" +
                            "${AuthenticatedBrowserTunnel.AUTH_HEADER}: " +
                            "${tunnel.lease.bearerToken}\r\n\r\n"
                        ).toByteArray(StandardCharsets.ISO_8859_1),
                )
                client.getOutputStream().flush()
                client.getInputStream().readBytes().toString(StandardCharsets.ISO_8859_1)
            }
            val forwarded = received.get(IO_TIMEOUT_MS.toLong(), TimeUnit.MILLISECONDS)

            assertTrue(response.startsWith("HTTP/1.1 200 OK"))
            assertTrue(response.endsWith("[]"))
            assertTrue(forwarded.startsWith("GET /json/list HTTP/1.1"))
            assertFalse(forwarded.contains(AuthenticatedBrowserTunnel.AUTH_HEADER, true))
            assertFalse(forwarded.contains(tunnel.lease.bearerToken))
        } finally {
            tunnel.close()
            upstreamServer.close()
            upstreamThread.join(IO_TIMEOUT_MS.toLong())
        }
    }

    private fun readHeader(socket: Socket): String {
        socket.soTimeout = IO_TIMEOUT_MS
        val bytes = ArrayList<Byte>()
        var suffix = ""
        while (!suffix.endsWith("\r\n\r\n")) {
            val next = socket.getInputStream().read()
            check(next >= 0) { "Unexpected end of proxied request" }
            bytes += next.toByte()
            suffix = (suffix + next.toChar()).takeLast(4)
        }
        return bytes.toByteArray().toString(StandardCharsets.ISO_8859_1)
    }

    private class SocketBrowserTunnelUpstream(
        private val socket: Socket,
    ) : BrowserTunnelUpstream {
        override val input = socket.getInputStream()
        override val output = socket.getOutputStream()
        override fun close() = socket.close()
    }

    private companion object {
        const val IO_TIMEOUT_MS = 5_000
    }
}
