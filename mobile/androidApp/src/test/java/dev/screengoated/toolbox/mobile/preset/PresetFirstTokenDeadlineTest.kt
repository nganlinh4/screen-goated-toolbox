package dev.screengoated.toolbox.mobile.preset

import kotlinx.coroutines.Job
import okhttp3.OkHttpClient
import okhttp3.Request
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.IOException
import java.net.ServerSocket
import java.net.Socket
import java.net.SocketTimeoutException
import java.util.concurrent.TimeUnit
import kotlin.concurrent.thread

class PresetFirstTokenDeadlineTest {
    private fun withServer(block: (Socket) -> Unit, read: (String) -> Unit) {
        ServerSocket(0).use { server ->
            val worker = thread(isDaemon = true) {
                server.accept().use { socket ->
                    val reader = socket.getInputStream().bufferedReader()
                    while (!reader.readLine().isNullOrEmpty()) { /* Consume request headers. */ }
                    runCatching { block(socket) }
                }
            }
            try { read("http://127.0.0.1:${server.localPort}/stream") }
            finally { worker.join(2_000) }
        }
    }

    private fun Socket.send(value: String) {
        getOutputStream().write(value.toByteArray())
        getOutputStream().flush()
    }

    private fun Socket.headers() = send("HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n")

    private fun consume(url: String, job: Job? = null, onOutput: () -> Unit = {}): String {
        val deadline = PresetFirstTokenDeadline(150, job)
        val request = Request.Builder().url(url)
            .tag(PresetFirstTokenDeadline::class.java, deadline).build()
        val state = PresetTransportState(false, deadline)
        val client = OkHttpClient.Builder().eventListener(state)
            .readTimeout(0, TimeUnit.MILLISECONDS).build()
        return PresetCall(client.newCall(request), state, deadline).execute().use { response ->
            buildString {
                response.body.charStream().buffered().forEachLine { line ->
                    if (line.startsWith("data: ")) {
                        val delta = extractOpenAiDelta(line.removePrefix("data: "))
                        if (delta.content.isNotEmpty()) {
                            response.firstOutputReceived()
                            append(delta.content)
                            onOutput()
                        }
                    }
                }
            }
        }
    }

    @Test
    fun outputContinuesAcrossGapsLongerThanInitialDeadline() = withServer({ socket ->
        socket.headers()
        socket.send(TOKEN)
        Thread.sleep(450)
        socket.send(TOKEN)
    }) { url -> assertEquals("xx", consume(url)) }

    @Test
    fun headersKeepalivesAndReasoningDoNotSatisfyDeadline() = withServer({ socket ->
        socket.headers()
        repeat(20) {
            socket.send(": keepalive\n\ndata: {\"choices\":[{\"delta\":{\"reasoning\":\"thinking\"}}]}\n\n")
            Thread.sleep(35)
        }
    }) { url ->
        assertTrue(runCatching { consume(url) }.exceptionOrNull() is SocketTimeoutException)
    }

    @Test
    fun missingHeadersStillTimeOut() = withServer({ Thread.sleep(450) }) { url ->
        assertTrue(runCatching { consume(url) }.exceptionOrNull() is SocketTimeoutException)
    }

    @Test
    fun cancellationInterruptsStalledReadAfterOutput() = withServer({ socket ->
        socket.headers()
        socket.send(TOKEN)
        Thread.sleep(450)
    }) { url ->
        val job = Job()
        val started = System.nanoTime()
        val error = runCatching { consume(url, job) { job.cancel() } }.exceptionOrNull()
        assertTrue(error is IOException)
        assertTrue(TimeUnit.NANOSECONDS.toMillis(System.nanoTime() - started) < 400)
    }

    companion object {
        private const val TOKEN = "data: {\"choices\":[{\"delta\":{\"content\":\"x\"}}]}\n\n"
    }
}
