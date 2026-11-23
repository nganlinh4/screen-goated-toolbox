package dev.screengoated.toolbox.mobile.phonecontrol.provider.browser

import java.io.File
import java.net.InetAddress
import java.net.InetSocketAddress
import java.net.Proxy
import java.net.ServerSocket
import java.net.SocketTimeoutException
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import kotlin.concurrent.thread
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import okhttp3.Dns
import okhttp3.HttpUrl.Companion.toHttpUrl
import okhttp3.OkHttpClient
import okhttp3.Request
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Test

class PublicResearchRedirectsTest {
    private val loopback = InetAddress.getByName("127.0.0.1")
    private val client = OkHttpClient.Builder()
        .dns(Dns { listOf(loopback) })
        .proxy(Proxy.NO_PROXY)
        // The helper must override even a client configured to follow redirects.
        .followRedirects(true)
        .followSslRedirects(true)
        .callTimeout(3, TimeUnit.SECONDS)
        .build()

    @Test
    fun sharedDestinationsAreValidatedBeforeDispatch() {
        val fixture = Json.parseToJsonElement(fixtureFile().readText()).jsonObject
        assertEquals(MAX_PUBLIC_RESEARCH_REDIRECTS, fixture.getValue("maximum_redirects").jsonPrimitive.content.toInt())
        assertEquals("local_public_only_no_proxy", fixture.getValue("destination_dns_owner").jsonPrimitive.content)
        for (case in fixture.getValue("cases").jsonArray) {
            val values = case.jsonObject
            val from = values.getValue("from").jsonPrimitive.content.toHttpUrl()
            val target = from.resolve(values.getValue("location").jsonPrimitive.content)
            val allowed = target != null && safePublicResearchUrl(target)
            assertEquals(values.toString(), values.getValue("allowed").jsonPrimitive.content.toBoolean(), allowed)
        }
    }

    @Test
    fun redirectToLoopbackNeverConnectsToPrivateServer() {
        ServerSocket(0, 10, loopback).use { privateServer ->
            ServerSocket(0, 10, loopback).use { publicServer ->
                val serving = serve(publicServer, 1) { _, _ ->
                    redirect("http://127.0.0.1:${privateServer.localPort}/private")
                }
                val request = Request.Builder().url("http://public.example:${publicServer.localPort}/start").build()
                assertNull(executePublicResearchGet(client, request) { true })
                serving.join(3_000)
                assertFalse(serving.isAlive)
                privateServer.soTimeout = 300
                val received = try { privateServer.accept().close(); true } catch (_: SocketTimeoutException) { false }
                assertFalse("private server received a request", received)
            }
        }
    }

    @Test
    fun configuredProxyCannotResolveDestinationsOutsideThePublicDnsPolicy() {
        ServerSocket(0, 10, loopback).use { proxy ->
            ServerSocket(0, 10, loopback).use { destination ->
                val serving = serve(destination, 1) { _, _ -> ok("direct") }
                val proxiedClient = client.newBuilder()
                    .proxy(Proxy(Proxy.Type.HTTP, InetSocketAddress(loopback, proxy.localPort)))
                    .build()
                val request = Request.Builder().url("http://public.example:${destination.localPort}/start").build()
                executePublicResearchGet(proxiedClient, request) { true }.use { response ->
                    assertEquals("direct", response?.body?.string())
                }
                serving.join(3_000)
                assertFalse(serving.isAlive)
                proxy.soTimeout = 300
                val usedProxy = try { proxy.accept().close(); true } catch (_: SocketTimeoutException) { false }
                assertFalse("the proxy received the research request", usedProxy)
            }
        }
    }

    @Test
    fun relativePublicRedirectsWorkAndRestrictedHostsStopBeforeDispatch() {
        ServerSocket(0, 10, loopback).use { server ->
            val count = AtomicInteger()
            val serving = serve(server, 2) { _, path ->
                count.incrementAndGet()
                if (path == "/start") redirect("/next") else ok("accepted")
            }
            val request = Request.Builder().url("http://public.example:${server.localPort}/start").build()
            executePublicResearchGet(client, request) { it.host == "public.example" }.use { response ->
                assertEquals(200, response?.code)
                assertEquals("accepted", response?.body?.string())
            }
            serving.join(3_000)
            assertEquals(2, count.get())
        }
        ServerSocket(0, 10, loopback).use { server ->
            val serving = serve(server, 1) { _, _ -> redirect("http://other.example:${server.localPort}/next") }
            val request = Request.Builder().url("http://public.example:${server.localPort}/start").build()
            assertNull(executePublicResearchGet(client, request) { it.host == "public.example" })
            serving.join(3_000)
            server.soTimeout = 300
            val secondRequest = try { server.accept().close(); true } catch (_: SocketTimeoutException) { false }
            assertFalse("restricted redirect was dispatched", secondRequest)
        }
    }

    @Test
    fun redirectCyclesAndLongChainsStopAtTheBound() {
        ServerSocket(0, 10, loopback).use { server ->
            val count = AtomicInteger()
            val serving = serve(server, MAX_PUBLIC_RESEARCH_REDIRECTS + 1) { _, _ ->
                redirect("/hop${count.incrementAndGet()}")
            }
            val request = Request.Builder().url("http://public.example:${server.localPort}/start").build()
            assertNull(executePublicResearchGet(client, request) { true })
            serving.join(3_000)
            assertEquals(MAX_PUBLIC_RESEARCH_REDIRECTS + 1, count.get())
        }
        ServerSocket(0, 10, loopback).use { server ->
            val serving = serve(server, 1) { _, _ -> redirect("/start#same-document") }
            val request = Request.Builder().url("http://public.example:${server.localPort}/start").build()
            assertNull(executePublicResearchGet(client, request) { true })
            serving.join(3_000)
            server.soTimeout = 300
            val secondRequest = try { server.accept().close(); true } catch (_: SocketTimeoutException) { false }
            assertFalse("redirect cycle was dispatched twice", secondRequest)
        }
    }

    private fun serve(server: ServerSocket, requests: Int, reply: (Int, String) -> String) = thread(isDaemon = true) {
        repeat(requests) { index ->
            server.accept().use { socket ->
                val reader = socket.getInputStream().bufferedReader()
                val path = reader.readLine().split(' ')[1]
                while (!reader.readLine().isNullOrEmpty()) {}
                socket.getOutputStream().write(reply(index, path).toByteArray(Charsets.UTF_8))
            }
        }
    }

    private fun redirect(location: String) =
        "HTTP/1.1 302 Found\r\nLocation: $location\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"

    private fun ok(body: String) =
        "HTTP/1.1 200 OK\r\nContent-Length: ${body.toByteArray(Charsets.UTF_8).size}\r\nConnection: close\r\n\r\n$body"

    private fun fixtureFile(): File {
        var directory: File? = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (directory != null) {
            val candidate = File(directory, "parity-fixtures/phone-control/public-research-redirects.json")
            if (candidate.isFile) return candidate
            directory = directory.parentFile
        }
        error("public research redirect fixture not found")
    }
}
