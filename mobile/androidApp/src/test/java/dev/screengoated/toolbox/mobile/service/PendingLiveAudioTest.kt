package dev.screengoated.toolbox.mobile.service

import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveBackoffPolicy
import java.io.File
import java.io.IOException
import java.util.concurrent.LinkedBlockingDeque
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import okhttp3.OkHttpClient
import org.junit.Test

class PendingLiveAudioTest {
    private val fixture by lazy {
        val path = "parity-fixtures/gemini-live-session/transcription-send-recovery.json"
        val file = generateSequence(File(".").absoluteFile) { it.parentFile }
            .map { File(it, path) }.first(File::isFile)
        Json.parseToJsonElement(file.readText()).jsonObject
    }

    @Test fun boundedQueuePreservesUnattemptedAudioBeforeConcurrentNewerSamples() {
        assertEquals(MAX_PENDING_LIVE_SAMPLES, fixture.getValue("maxPendingSamples").jsonPrimitive.int)
        for (case in fixture.getValue("cases").jsonArray) {
            val item = case.jsonObject
            fun audio(name: String) = item.getValue(name).jsonArray.map { it.jsonPrimitive.int.toShort() }.toShortArray()
            val queue = PendingLiveAudio(item.getValue("capacity").jsonPrimitive.int)
            var dropped = queue.append(audio("queue"))
            dropped += queue.prepend(audio("unattempted"))
            dropped += queue.append(audio("newer"))
            assertEquals(item.getValue("dropped").jsonPrimitive.int, dropped)
            assertArrayEquals(audio("expected"), queue.take(Int.MAX_VALUE))
        }
    }

    @Test fun failedFrameIsNotReplayedAndLaterFramesStayOrdered() {
        val attempted = mutableListOf<List<Short>>()
        val input = shortArrayOf(1, 2, 3, 4, 5, 6, 7)
        val end = sendLiveAudioFrames(input, 2) {
            attempted += it.toList()
            attempted.size != 2
        }!!
        assertEquals(listOf(listOf<Short>(1, 2), listOf<Short>(3, 4)), attempted)
        val queue = PendingLiveAudio(8)
        queue.append(shortArrayOf(8, 9))
        queue.prepend(input.copyOfRange(end, input.size))
        assertArrayEquals(shortArrayOf(5, 6, 7, 8, 9), queue.take(8))
    }

    @Test fun retryScheduleMatchesTheSharedContract() {
        assertEquals(6, fixture.getValue("maxReconnectAttempts").jsonPrimitive.int)
        for (entry in fixture.getValue("attempts").jsonArray) {
            val attempt = entry.jsonObject
            assertEquals(attempt.getValue("delayMs").jsonPrimitive.int.toLong(),
                GeminiLiveBackoffPolicy().delayMs(attempt.getValue("attempt").jsonPrimitive.int))
        }
    }

    @Test fun rejectedResumptionIsClearedBeforeFallbackAndLaterReconnect() = runBlocking {
        val resumption = LiveResumption(fixture.getValue("rejectedHandle").jsonPrimitive.content)
        val submitted = mutableListOf<String?>()
        val first = reconnectLive(resumption, 3, { false }, connect = { handle ->
            submitted += handle
            if (handle != null) throw LiveSetupFailure("rejected", false)
            "connected"
        }, pause = {})
        assertEquals("connected", first)
        assertEquals(listOf("expired-session", null), submitted)
        assertNull(resumption.handle)
        reconnectLive(resumption, 3, { false }, connect = { handle ->
            submitted += handle
            "connected again"
        }, pause = {})
        assertEquals(listOf("expired-session", null, null), submitted)
    }

    @Test fun transcriptInRotationFrameIsQueuedBeforeGoAway() {
        val queue = LinkedBlockingDeque<GeminiLiveSocketClient.LiveSocketEvent>()
        GeminiLiveSocketClient(OkHttpClient()).handleMessage(
            fixture.getValue("frameWithGoAway").toString(), queue, CompletableDeferred(),
        )
        val events = queue.filter { it is GeminiLiveSocketClient.LiveSocketEvent.Transcript || it is GeminiLiveSocketClient.LiveSocketEvent.GoAway }
        assertEquals(listOf(GeminiLiveSocketClient.LiveSocketEvent.Transcript("source text", true),
            GeminiLiveSocketClient.LiveSocketEvent.GoAway), events)
        queue.clear()
        val setupReady = CompletableDeferred<Unit>()
        var eventsAtSetup = emptyList<GeminiLiveSocketClient.LiveSocketEvent>()
        setupReady.invokeOnCompletion { eventsAtSetup = queue.toList() }
        GeminiLiveSocketClient(OkHttpClient()).handleMessage(
            fixture.getValue("setupFrameWithGoAway").toString(), queue, setupReady,
        )
        val interimEvents = queue.filter { it is GeminiLiveSocketClient.LiveSocketEvent.Transcript || it is GeminiLiveSocketClient.LiveSocketEvent.GoAway }
        assertEquals(listOf(GeminiLiveSocketClient.LiveSocketEvent.Transcript("draft", false),
            GeminiLiveSocketClient.LiveSocketEvent.GoAway), interimEvents)
        assertEquals(queue.toList(), eventsAtSetup)
    }

    @Test fun failedSendDeliversQueuedTranscriptAndResumptionBeforeReconnect() = runBlocking {
        val queue = LinkedBlockingDeque<GeminiLiveSocketClient.LiveSocketEvent>()
        queue.add(GeminiLiveSocketClient.LiveSocketEvent.Transcript("received", true))
        queue.add(GeminiLiveSocketClient.LiveSocketEvent.GoAway)
        queue.add(GeminiLiveSocketClient.LiveSocketEvent.Resumption("fresh"))
        val sequence = mutableListOf<String>()
        recoverRetiredLiveSession(queue,
            onTranscript = { text, _ -> sequence.add("transcript:$text") },
            onResumption = { sequence.add("resumption:$it") },
            onProgress = {},
            reconnect = { sequence.add("reconnect"); Unit },
        )
        assertEquals(listOf("transcript:received", "resumption:fresh", "reconnect"), sequence)
    }

    @Test fun captureEndingDuringSetupStopsFurtherAttempts() = runBlocking {
        var ended = false
        var attempts = 0
        var pauses = 0
        val producer = launch { delay(10); ended = true }
        val result = reconnectLive(LiveResumption(), 3, { ended }, connect = {
            attempts++
            if (awaitLiveSetup(CompletableDeferred(), { ended })) "connected" else null
        }, pause = { pauses++ })
        producer.join()
        assertNull(result)
        assertEquals(1, attempts)
        assertEquals(0, pauses)
    }

    @Test fun retirementWaitsForAdmittedFrameAndRejectsLateCallbacks() = runBlocking {
        val inbox = LiveEventInbox()
        val started = CountDownLatch(1)
        val release = CountDownLatch(1)
        val retired = CountDownLatch(1)
        val callback = Thread {
            inbox.admit {
                started.countDown()
                release.await(2, TimeUnit.SECONDS)
                inbox.events.add(GeminiLiveSocketClient.LiveSocketEvent.Transcript("in flight", true))
                inbox.events.add(GeminiLiveSocketClient.LiveSocketEvent.GoAway)
            }
        }
        callback.start()
        assertTrue(started.await(2, TimeUnit.SECONDS))
        val retire = Thread { inbox.retire(); retired.countDown() }
        retire.start()
        try {
            assertFalse(retired.await(50, TimeUnit.MILLISECONDS))
        } finally {
            release.countDown()
        }
        callback.join(2_000)
        retire.join(2_000)
        assertTrue(retired.await(2, TimeUnit.SECONDS))
        inbox.admit { inbox.events.add(GeminiLiveSocketClient.LiveSocketEvent.Transcript("late", true)) }
        val sequence = mutableListOf<String>()
        recoverRetiredLiveSession(inbox.events,
            onTranscript = { text, _ -> sequence.add(text) },
            onResumption = {}, onProgress = {},
            reconnect = { sequence.add("reconnected"); Unit },
        )
        assertEquals(listOf("in flight", "reconnected"), sequence)
    }

    @Test fun captureEndingInterruptsReconnectBackoff() = runBlocking {
        var ended = false
        var attempts = 0
        val producer = launch { delay(10); ended = true }
        val started = System.nanoTime()
        val result = reconnectLive(LiveResumption(), 3, { ended }, connect = {
            attempts++
            null
        }, pause = { delayWhileCaptureActive(4_000, { ended }) })
        producer.join()
        assertNull(result)
        assertEquals(1, attempts)
        assertTrue(TimeUnit.NANOSECONDS.toMillis(System.nanoTime() - started) < 2_000)
    }

    @Test fun setupCloseAndRejectedWriteFailImmediatelyWithoutChangingActiveClose() = runBlocking {
        for (case in fixture.getValue("preSetupFailures").jsonArray) {
            val setup = CompletableDeferred<Unit>()
            val events = LinkedBlockingDeque<GeminiLiveSocketClient.LiveSocketEvent>()
            when (case.jsonPrimitive.content) {
                "closed" -> closeLiveSocket(setup, events)
                "setup-send-rejected" -> sendLiveSetup(setup) { false }
                else -> error("unknown fixture case")
            }
            assertTrue(setup.isCompleted)
            assertTrue(runCatching { awaitLiveSetup(setup) { false } }.exceptionOrNull() is IOException)
        }
        val accepted = CompletableDeferred<Unit>()
        sendLiveSetup(accepted) { true }
        assertFalse(accepted.isCompleted)
        accepted.complete(Unit)
        val events = LinkedBlockingDeque<GeminiLiveSocketClient.LiveSocketEvent>()
        closeLiveSocket(accepted, events)
        assertTrue(awaitLiveSetup(accepted) { false })
        assertEquals(GeminiLiveSocketClient.LiveSocketEvent.Closed, events.poll())
    }
}
