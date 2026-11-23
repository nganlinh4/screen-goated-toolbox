package dev.screengoated.toolbox.mobile.shared.live

import java.io.File
import java.net.ConnectException
import java.net.InetAddress
import java.net.InetSocketAddress
import java.net.ServerSocket
import java.net.Socket
import java.net.SocketAddress
import java.nio.charset.StandardCharsets
import java.security.MessageDigest
import java.util.Base64
import java.util.Collections
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicReference
import javax.net.SocketFactory
import okhttp3.Dns
import okhttp3.OkHttpClient
import okhttp3.Request
import okio.ByteString
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import kotlin.concurrent.thread

class GeminiLiveRouteFallbackTest {
    @Test
    fun connectorRetriesNextLoopbackAddressAfterFirstRouteFails() {
        var root = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (!File(root, "parity-fixtures/gemini-live-session/transport-address-fallback.json").isFile) {
            root = root.parentFile ?: error("fixture missing")
        }
        val fixture = JSONObject(File(root, "parity-fixtures/gemini-live-session/transport-address-fallback.json").readText())
        val expectedAttempts = fixture.getJSONArray("cases").getJSONObject(1).getInt("expectedAttempts")
        val first = InetAddress.getByAddress(byteArrayOf(127, 0, 0, 2))
        val second = InetAddress.getByAddress(byteArrayOf(127, 0, 0, 1))
        val attempts = Collections.synchronizedList(mutableListOf<InetAddress>())
        val server = ServerSocket(0, 1, second).apply { soTimeout = 5_000 }
        val serverError = AtomicReference<Throwable?>(null)
        val serverThread = thread(name = "live-route-test") {
            try {
                server.accept().use { socket ->
                    socket.soTimeout = 5_000
                    val reader = socket.getInputStream().bufferedReader(StandardCharsets.US_ASCII)
                    var key: String? = null
                    while (true) {
                        val line = reader.readLine() ?: error("incomplete WebSocket request")
                        if (line.isEmpty()) break
                        if (line.startsWith("Sec-WebSocket-Key:", ignoreCase = true)) key = line.substringAfter(':').trim()
                    }
                    val accept = Base64.getEncoder().encodeToString(MessageDigest.getInstance("SHA-1").digest(
                        (requireNotNull(key) + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11").toByteArray(StandardCharsets.US_ASCII),
                    ))
                    socket.getOutputStream().write("HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: $accept\r\n\r\n".toByteArray(StandardCharsets.US_ASCII))
                    socket.getOutputStream().flush()
                    Thread.sleep(100)
                }
            } catch (error: Throwable) { serverError.set(error) }
        }
        val factory = object : SocketFactory() {
            override fun createSocket(): Socket = object : Socket() {
                override fun connect(endpoint: SocketAddress, timeout: Int) {
                    val address = (endpoint as InetSocketAddress).address
                    require(address.isLoopbackAddress)
                    attempts.add(address)
                    if (address == first) throw ConnectException("injected first-route failure")
                    super.connect(endpoint, timeout)
                }
            }
            override fun createSocket(host: String, port: Int): Socket = error("unexpected socket overload")
            override fun createSocket(host: String, port: Int, localHost: InetAddress, localPort: Int): Socket = error("unexpected socket overload")
            override fun createSocket(host: InetAddress, port: Int): Socket = error("unexpected socket overload")
            override fun createSocket(address: InetAddress, port: Int, localAddress: InetAddress, localPort: Int): Socket = error("unexpected socket overload")
        }
        val client = OkHttpClient.Builder()
            .dns(Dns { listOf(first, second) })
            .socketFactory(factory)
            .retryOnConnectionFailure(true)
            .build()
        val opened = CountDownLatch(1)
        val failed = AtomicReference<Throwable?>(null)
        val socket = OkHttpGeminiLiveSocketConnector(client, Request.Builder().url("ws://route.invalid:${server.localPort}/live").build()).connect(
            object : GeminiLiveSocketListener {
                override fun onOpen() { opened.countDown() }
                override fun onText(payload: String) = Unit
                override fun onBinary(payload: ByteString) = Unit
                override fun onClosing(code: Int, reason: String) = Unit
                override fun onClosed(code: Int, reason: String) = Unit
                override fun onFailure(error: Throwable) { failed.set(error); opened.countDown() }
            },
        )
        try {
            assertTrue("WebSocket callback timed out", opened.await(5, TimeUnit.SECONDS))
            assertNull(failed.get())
            assertEquals(expectedAttempts, attempts.size)
            assertEquals(listOf(first, second), attempts.toList())
        } finally {
            socket.cancel()
            server.close()
            serverThread.join(5_000)
            client.dispatcher.executorService.shutdownNow()
            client.connectionPool.evictAll()
        }
        assertFalse(serverThread.isAlive)
        assertNull(serverError.get())
    }
}
