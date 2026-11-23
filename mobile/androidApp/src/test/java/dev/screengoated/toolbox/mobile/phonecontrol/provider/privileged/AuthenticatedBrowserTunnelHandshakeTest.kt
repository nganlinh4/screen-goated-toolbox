package dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged

import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.io.File
import java.io.InputStream
import java.io.OutputStream
import java.net.ConnectException
import java.net.InetAddress
import java.net.Socket
import java.nio.charset.StandardCharsets
import java.util.concurrent.ConcurrentLinkedDeque
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class AuthenticatedBrowserTunnelHandshakeTest {
    @Test
    fun headerBudgetIsOneDeadlineForTheWholeHeader() {
        val deadline = 10_000_000_000L
        assertEquals(5_000, AuthenticatedBrowserTunnel.headerReadTimeoutMs(deadline, deadline - 5_000_000_000L))
        assertEquals(1, AuthenticatedBrowserTunnel.headerReadTimeoutMs(deadline, deadline - 1L))
        assertNull(AuthenticatedBrowserTunnel.headerReadTimeoutMs(deadline, deadline))
        assertNull(AuthenticatedBrowserTunnel.headerReadTimeoutMs(deadline, deadline + 1L))
    }

    @Test
    fun silentLocalSocketsCannotLockOutAnAuthenticatedClient() {
        val tunnel = AuthenticatedBrowserTunnel(openUpstream = ::CannedUpstream)
        val loopback = InetAddress.getByName(AuthenticatedBrowserTunnel.LOOPBACK_ADDRESS)
        val squatters = List(AuthenticatedBrowserTunnel.MAX_PENDING_HANDSHAKES + 4) {
            connect(loopback, tunnel.lease.port).apply { getOutputStream().write('G'.code) }
        }
        try {
            connect(loopback, tunnel.lease.port).use { client ->
                client.soTimeout = 5_000
                client.getOutputStream().write(
                    (
                        "GET /json/version HTTP/1.1\r\n" +
                            "Host: 127.0.0.1\r\n" +
                            "${AuthenticatedBrowserTunnel.AUTH_HEADER}: ${tunnel.lease.bearerToken}\r\n" +
                            "\r\n"
                        ).toByteArray(StandardCharsets.ISO_8859_1),
                )
                client.getOutputStream().flush()
                val response = client.getInputStream().readBytes()
                    .toString(StandardCharsets.ISO_8859_1)
                assertTrue(response, response.startsWith("HTTP/1.1 200 OK"))
            }
        } finally {
            squatters.forEach { runCatching(it::close) }
            tunnel.close()
        }
    }

    @Test
    fun closeSurvivesHandshakesLeavingDuringTeardown() {
        // A serving thread removes its socket between size() and iteration:
        // the deque reports one entry but iterates none. toList() threw here.
        val shrinking = object : ConcurrentLinkedDeque<Socket>() {
            override val size: Int get() = 1
        }
        closeDrained(shrinking)

        val sockets = ConcurrentLinkedDeque(List(3) { Socket() })
        closeDrained(sockets)
        assertTrue(sockets.isEmpty())

        val source = tunnelSource()
        val close = source.substringAfter("override fun close()").substringBefore("private fun acceptLoop()")
        assertTrue(close.contains("closeDrained(handshakes)"))
        assertFalse(close.contains(".toList()"))
    }

    @Test
    fun executorSubmissionAfterCloseIsGuarded() {
        val acceptLoop = tunnelSource().substringAfter("private fun acceptLoop()")
            .substringBefore("private fun admitHandshake(")
        assertTrue(acceptLoop.contains("catch (_: RejectedExecutionException)"))
        val tunnel = AuthenticatedBrowserTunnel(openUpstream = ::CannedUpstream)
        tunnel.close()
        tunnel.close()
        assertFalse(tunnel.isOpen)
    }

    private fun tunnelSource(): String {
        var directory = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (!File(directory, ".claude/parity").exists()) {
            directory = requireNotNull(directory.parentFile)
        }
        return File(
            directory,
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/phonecontrol/" +
                "provider/privileged/AuthenticatedBrowserTunnel.kt",
        ).readText()
    }

    /**
     * A burst of connects can outrun the accept thread. Windows refuses a
     * connect once the listen backlog is full (Linux retries the SYN), so the
     * squatters retry briefly instead of failing before the tunnel is tested.
     */
    private fun connect(address: InetAddress, port: Int): Socket {
        repeat(CONNECT_ATTEMPTS - 1) {
            try {
                return Socket(address, port)
            } catch (_: ConnectException) {
                Thread.sleep(CONNECT_RETRY_MS)
            }
        }
        return Socket(address, port)
    }

    private companion object {
        const val CONNECT_ATTEMPTS = 50
        const val CONNECT_RETRY_MS = 20L
    }

    private class CannedUpstream : BrowserTunnelUpstream {
        override val input: InputStream = ByteArrayInputStream(
            "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}".toByteArray(StandardCharsets.ISO_8859_1),
        )
        override val output: OutputStream = ByteArrayOutputStream()

        override fun close() = Unit
    }
}
