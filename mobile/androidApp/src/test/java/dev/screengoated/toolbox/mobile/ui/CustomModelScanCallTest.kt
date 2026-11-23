package dev.screengoated.toolbox.mobile.ui

import java.io.IOException
import java.nio.file.Files
import java.nio.file.Paths
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.async
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.supervisorScope
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import okhttp3.Call
import okhttp3.Callback
import okhttp3.MediaType
import okhttp3.OkHttpClient
import okhttp3.Protocol
import okhttp3.Request
import okhttp3.Response
import okhttp3.ResponseBody
import okio.Buffer
import okio.BufferedSource
import okio.ForwardingSource
import okio.buffer
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class CustomModelScanCallTest {
    @Test fun sharedCancellationCasesReplayAgainstCallHandoff() = runBlocking {
        val path = listOf(Paths.get("..", "parity-fixtures"), Paths.get("..", "..", "parity-fixtures"),
            Paths.get("parity-fixtures")).map { it.resolve("preset-system/custom-model-scan-cancellation.json") }.first(Files::exists)
        val cases = Json.parseToJsonElement(Files.readAllBytes(path).decodeToString()).jsonObject.getValue("cases").jsonArray
        supervisorScope {
            for (entry in cases) {
                val case = entry.jsonObject
                val event = case.getValue("event").jsonPrimitive.content
                val call = ControlledCall()
                var parsed = false
                val pending = async(start = CoroutineStart.UNDISPATCHED) {
                    readScanResponse(call) {
                        parsed = true
                        if (event == "response_parse_failure") throw IOException("Controlled parse failure")
                        it.body.string()
                    }
                }
                assertTrue(call.started.await(5, TimeUnit.SECONDS))
                val outcome = when (event) {
                    "cancel_in_flight" -> {
                        pending.cancelAndJoin(); check(call.cancelled.get()); "call_cancelled"
                    }
                    "response_after_cancellation" -> {
                        pending.cancelAndJoin()
                        val body = TrackingBody("late")
                        call.respond(body)
                        check(body.closed.get() && !parsed); "closed_without_delivery"
                    }
                    "cancel_during_response_read" -> {
                        val readStarted = CountDownLatch(1)
                        val readRelease = CountDownLatch(1)
                        val body = TrackingBody("blocked", readStarted, readRelease)
                        val worker = Thread { call.respond(body) }.apply { start() }
                        try {
                            check(readStarted.await(5, TimeUnit.SECONDS))
                            pending.cancelAndJoin()
                            check(call.cancelled.get())
                        } finally {
                            readRelease.countDown()
                            worker.join(5_000)
                        }
                        check(!worker.isAlive && body.closed.get()); "call_cancelled_and_response_closed"
                    }
                    "failure_after_cancellation" -> {
                        pending.cancelAndJoin()
                        call.fail()
                        check(!parsed); "ignored"
                    }
                    "response_parse_failure" -> {
                        val body = TrackingBody("payload")
                        call.respond(body)
                        check(runCatching { pending.await() }.exceptionOrNull() is IOException && body.closed.get())
                        "failure_delivered_and_closed"
                    }
                    "successful_response" -> {
                        val body = TrackingBody("ok")
                        call.respond(body)
                        check(pending.await() == "ok" && body.closed.get()); "delivered_and_closed"
                    }
                    "request_failure" -> {
                        call.fail()
                        check(runCatching { pending.await() }.exceptionOrNull() is IOException); "failure_delivered"
                    }
                    else -> error("Unknown scan cancellation fixture case")
                }
                assertEquals(event, case.getValue("result").jsonPrimitive.content, outcome)
            }
        }
    }

    @Test fun cancelInFlightCancelsTheUnderlyingCall() = runBlocking {
        val call = ControlledCall()
        val job = async(start = CoroutineStart.UNDISPATCHED) { readScanResponse(call) { it.body.string() } }
        try {
            assertTrue(call.started.await(5, TimeUnit.SECONDS))
            job.cancel()
            assertTrue("Cancellation did not reach the HTTP call", call.cancelled.get())
        } finally {
            call.release.countDown()
            job.cancelAndJoin()
        }
    }

    @Test fun successClosesResponseAndLateResponseAfterCancellationIsClosed() = runBlocking {
        val successCall = ControlledCall()
        val success = async(start = CoroutineStart.UNDISPATCHED) { readScanResponse(successCall) { it.body.string() } }
        assertTrue(successCall.started.await(5, TimeUnit.SECONDS))
        val successBody = TrackingBody("ok")
        successCall.respond(successBody)
        assertEquals("ok", success.await())
        assertTrue(successBody.closed.get())

        val lateCall = ControlledCall()
        val late = async(start = CoroutineStart.UNDISPATCHED) { readScanResponse(lateCall) { it.body.string() } }
        assertTrue(lateCall.started.await(5, TimeUnit.SECONDS))
        late.cancelAndJoin()
        val lateBody = TrackingBody("late")
        lateCall.respond(lateBody)
        assertTrue(lateBody.closed.get())
    }

    private class ControlledCall : Call by OkHttpClient().newCall(requestValue) {
        val started = CountDownLatch(1)
        val release = CountDownLatch(1)
        val cancelled = AtomicBoolean(false)
        private lateinit var callback: Callback

        override fun execute(): Response {
            started.countDown()
            release.await(5, TimeUnit.SECONDS)
            return Response.Builder().request(requestValue).protocol(Protocol.HTTP_1_1)
                .code(200).message("OK").body(TrackingBody("unused")).build()
        }

        override fun enqueue(responseCallback: Callback) { callback = responseCallback; started.countDown() }
        override fun cancel() { cancelled.set(true) }
        fun fail() = callback.onFailure(this, IOException("Controlled failure"))
        fun respond(body: ResponseBody) = callback.onResponse(this, Response.Builder().request(requestValue)
            .protocol(Protocol.HTTP_1_1).code(200).message("OK").body(body).build())
    }

    private class TrackingBody(
        value: String, private val readStarted: CountDownLatch? = null, private val readRelease: CountDownLatch? = null,
    ) : ResponseBody() {
        val closed = AtomicBoolean(false)
        private val input = object : ForwardingSource(Buffer().writeUtf8(value)) {
            override fun read(sink: Buffer, byteCount: Long): Long {
                readStarted?.countDown()
                readRelease?.await(5, TimeUnit.SECONDS)
                return super.read(sink, byteCount)
            }
            override fun close() { closed.set(true); super.close() }
        }.buffer()
        override fun contentType(): MediaType? = null
        override fun contentLength(): Long = -1
        override fun source(): BufferedSource = input
    }

    private companion object {
        val requestValue = Request.Builder().url("https://example.invalid/").build()
    }
}
