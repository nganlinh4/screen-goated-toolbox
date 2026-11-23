package dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged

import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.io.IOException
import java.io.InputStream
import java.io.OutputStream
import java.net.InetAddress
import java.net.Socket
import java.nio.charset.StandardCharsets
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class AuthenticatedBrowserTunnelKeepAliveTest {
    @Test
    fun aReusedConnectionNeverCarriesTheSecretToChrome() {
        val upstream = HoldingUpstream(OK_RESPONSE)
        val tunnel = AuthenticatedBrowserTunnel { upstream }
        val token = tunnel.lease.bearerToken
        try {
            connect(tunnel).use { client ->
                client.send(request("GET /json/list", token))
                assertEquals(OK_RESPONSE, client.receive(OK_RESPONSE.length))
                client.send(request("GET /json/version", token))
                assertTrue("a second request must end the connection", client.closedByPeer())
            }
            assertTrue(upstream.closed.await(IO_TIMEOUT_SECONDS, TimeUnit.SECONDS))
            val forwarded = upstream.forwardedText()
            assertEquals("GET /json/list HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n", forwarded)
            assertFalse(forwarded.contains(token))
            assertFalse(forwarded.contains(AuthenticatedBrowserTunnel.AUTH_HEADER, ignoreCase = true))
        } finally {
            tunnel.close()
        }
    }

    @Test
    fun theDeclaredBodyIsForwardedButNothingAfterIt() {
        val upstream = HoldingUpstream(OK_RESPONSE)
        val tunnel = AuthenticatedBrowserTunnel { upstream }
        val token = tunnel.lease.bearerToken
        try {
            connect(tunnel).use { client ->
                client.send(request("PUT /json/new", token, "Content-Length: 2\r\n") + "{}")
                assertEquals(OK_RESPONSE, client.receive(OK_RESPONSE.length))
                client.send(request("GET /json/list", token))
                assertTrue(client.closedByPeer())
            }
            assertTrue(upstream.closed.await(IO_TIMEOUT_SECONDS, TimeUnit.SECONDS))
            assertEquals(
                "PUT /json/new HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 2\r\n\r\n{}",
                upstream.forwardedText(),
            )
        } finally {
            tunnel.close()
        }
    }

    @Test
    fun anAcceptedWebSocketUpgradeKeepsStreaming() {
        val switching = "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\r\n"
        val upstream = HoldingUpstream(switching)
        val tunnel = AuthenticatedBrowserTunnel { upstream }
        try {
            connect(tunnel).use { client ->
                client.send(request("GET /devtools/page/A", tunnel.lease.bearerToken, "Upgrade: websocket\r\n"))
                assertEquals(switching, client.receive(switching.length))
                client.send("frame-bytes")
                val deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(IO_TIMEOUT_SECONDS)
                while (!upstream.forwardedText().endsWith("frame-bytes") && System.nanoTime() < deadline) {
                    Thread.sleep(10)
                }
                assertTrue(upstream.forwardedText().endsWith("\r\n\r\nframe-bytes"))
            }
        } finally {
            tunnel.close()
        }
    }

    @Test
    fun onlyUnambiguousRequestFramingIsForwardable() {
        assertEquals(0L, tunnelRequestBodyLength(header()))
        assertEquals(2L, tunnelRequestBodyLength(header("Content-Length: 2\r\n")))
        assertNull(tunnelRequestBodyLength(header("Transfer-Encoding: chunked\r\n")))
        assertNull(tunnelRequestBodyLength(header("Content-Length: 2\r\nContent-Length: 2\r\n")))
        assertNull(tunnelRequestBodyLength(header("Content-Length: -1\r\n")))
        assertNull(tunnelRequestBodyLength(header("Content-Length: +2\r\n")))
        assertNull(tunnelRequestBodyLength(header("Content-Length: 99999999\r\n")))
        assertTrue(isSwitchingProtocols(bytes("HTTP/1.1 101 Switching Protocols\r\n")))
        assertFalse(isSwitchingProtocols(bytes("HTTP/1.1 200 OK\r\n")))
        assertFalse(isSwitchingProtocols(bytes("HTTP/1.1 1010 Nope\r\n")))
    }

    @Test
    fun plainDevToolsCallsNeverAskToReuseATunnelConnection() {
        var directory = java.io.File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (!java.io.File(directory, ".claude/parity").exists()) {
            directory = requireNotNull(directory.parentFile)
        }
        val transport = java.io.File(
            directory,
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/phonecontrol/" +
                "provider/browser/AndroidChromeDevToolsTransport.kt",
        ).readText()
        val authenticated = Regex("\\.header\\(AUTH_HEADER, active\\.bearerToken\\)").findAll(transport).count()
        val closing = transport.split(".header(CONNECTION_HEADER, CONNECTION_CLOSE)").size - 1
        assertEquals("every HTTP call except the WebSocket upgrade closes", authenticated - 1, closing)
        assertTrue(transport.contains("const val CONNECTION_CLOSE = \"close\""))
    }

    /** Serves a canned response and then holds the connection open like a keep-alive Chrome. */
    private class HoldingUpstream(response: String) : BrowserTunnelUpstream {
        val closed = CountDownLatch(1)
        private val forwarded = ByteArrayOutputStream()
        private val canned = ByteArrayInputStream(bytes(response))

        override val input: InputStream = object : InputStream() {
            override fun read(): Int = canned.read().takeIf { it >= 0 } ?: endAfterClose()

            override fun read(buffer: ByteArray, offset: Int, length: Int): Int {
                if (length == 0) return 0
                return canned.read(buffer, offset, length).takeIf { it > 0 } ?: endAfterClose()
            }

            private fun endAfterClose(): Int {
                closed.await()
                return -1
            }
        }
        override val output: OutputStream = forwarded

        fun forwardedText(): String = forwarded.toByteArray().toString(StandardCharsets.ISO_8859_1)

        override fun close() = closed.countDown()
    }

    private companion object {
        const val IO_TIMEOUT_SECONDS = 5L
        const val OK_RESPONSE = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n[]"

        fun bytes(text: String): ByteArray = text.toByteArray(StandardCharsets.ISO_8859_1)

        fun header(extra: String = ""): ByteArray = bytes("GET /json/list HTTP/1.1\r\nHost: 127.0.0.1\r\n$extra\r\n")

        fun request(line: String, token: String, extra: String = ""): String =
            "$line HTTP/1.1\r\nHost: 127.0.0.1\r\n" +
                "${AuthenticatedBrowserTunnel.AUTH_HEADER}: $token\r\n$extra\r\n"

        fun connect(tunnel: AuthenticatedBrowserTunnel): Socket =
            Socket(InetAddress.getByName(AuthenticatedBrowserTunnel.LOOPBACK_ADDRESS), tunnel.lease.port)
                .apply { soTimeout = (IO_TIMEOUT_SECONDS * 1_000).toInt() }

        fun Socket.send(text: String) {
            getOutputStream().write(bytes(text))
            getOutputStream().flush()
        }

        fun Socket.receive(length: Int): String {
            val received = ByteArrayOutputStream()
            while (received.size() < length) {
                val next = getInputStream().read()
                if (next < 0) break
                received.write(next)
            }
            return received.toByteArray().toString(StandardCharsets.ISO_8859_1)
        }

        /** True when the tunnel ends the connection; a read timeout means it stayed open. */
        fun Socket.closedByPeer(): Boolean = try {
            getInputStream().read() < 0
        } catch (_: java.net.SocketTimeoutException) {
            false
        } catch (_: IOException) {
            true
        }
    }
}
