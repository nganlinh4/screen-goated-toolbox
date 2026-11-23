package dev.screengoated.toolbox.mobile.preset

import java.util.concurrent.LinkedBlockingDeque
import java.io.File
import org.json.JSONObject
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.job
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import okhttp3.Request
import okhttp3.WebSocket
import okio.ByteString
import org.junit.Test

class GeminiLivePresetWaitTest {
    private val contract = JSONObject(generateSequence(File(requireNotNull(System.getProperty("user.dir")))) { it.parentFile }
        .map { File(it, "parity-fixtures/preset-system/live-response-wait.json") }.first(File::isFile).readText())

    @Test fun sharedResponseTimelinesDriveTheActualPolicy() {
        val policy=GeminiLivePresetResponsePolicy.finite().copy(firstResponseTimeoutMs=contract.getLong("firstOutputTimeoutMillis"),contentIdleMs=contract.getLong("idleCompletionMillis"),hardResponseTimeoutMs=contract.getLong("hardCompletionMillis"))
        assertEquals(GeminiLivePresetResponsePolicy.finite(),policy)
        assertEquals(contract.getLong("maximumCancellationPollMillis"),GEMINI_LIVE_PRESET_CANCEL_POLL_MILLIS)
        val cases=contract.getJSONArray("cases")
        for(index in 0 until cases.length()) {
            val case=cases.getJSONObject(index)
            val state=GeminiLivePresetResponseState(policy,case.getBoolean("interactionIdle"),0)
            val events=case.getJSONArray("events")
            for(eventIndex in 0 until events.length()) {
                val event=events.getJSONObject(eventIndex)
                val now=event.getLong("atMs")
                val kind=event.getString("kind")
                if(kind=="text")state.observeText(now,event.getString("text"))
                val actual=state.outcome(now,kind=="complete",kind=="closed")
                val expected=when(event.getString("expected")) {
                    "pending"->GeminiLivePresetWaitOutcome.PENDING
                    "complete"->GeminiLivePresetWaitOutcome.COMPLETE
                    "first_output_timeout"->GeminiLivePresetWaitOutcome.FIRST_OUTPUT_TIMEOUT
                    "hard_timeout"->GeminiLivePresetWaitOutcome.HARD_TIMEOUT
                    "closed"->GeminiLivePresetWaitOutcome.CLOSED
                    else->error("Unknown fixture outcome")
                }
                assertEquals(case.getString("name"),expected,actual)
            }
            assertEquals(case.getString("name"),case.getBoolean("firstOutput"),state.contentStarted)
        }
    }

    @Test fun successfulAndFailedScopesReleaseTheirOwnedSocket()=runBlocking {
        val success=RecordingSocket()
        assertEquals("done",withOwnedGeminiLivePresetSocket(success){"done"})
        assertEquals(1,success.cancelled)
        val failure=RecordingSocket()
        val result=runCatching {withOwnedGeminiLivePresetSocket(failure){error("failed response")}}
        assertTrue(result.isFailure)
        assertEquals(1,failure.cancelled)
    }

    @Test fun cancellationReleasesTheSocketBeforeOutput()=runBlocking {
        val socket=RecordingSocket()
        val started=kotlinx.coroutines.CompletableDeferred<Unit>()
        val job=launch {
            withOwnedGeminiLivePresetSocket(socket) {
                started.complete(Unit)
                awaitGeminiLivePresetEvent(LinkedBlockingDeque(),contract.getLong("firstOutputTimeoutMillis"))
            }
        }
        started.await()
        withTimeout(500){job.cancelAndJoin()}
        assertEquals(1,socket.cancelled)
    }

    @Test fun cancellationDuringOutputDoesNotDeliverTheNextChunk()=runBlocking {
        val socket=RecordingSocket()
        val delivered=mutableListOf<String>()
        val job=launch {
            val owner=coroutineContext.job
            val events=LinkedBlockingDeque<GeminiLivePresetEvent>()
            events.add(GeminiLivePresetEvent.Chunk("first"))
            events.add(GeminiLivePresetEvent.Chunk("late"))
            withOwnedGeminiLivePresetSocket(socket) {
                repeat(2) {
                    val event=awaitGeminiLivePresetEvent(events,100) as GeminiLivePresetEvent.Chunk
                    delivered.add(event.text)
                    owner.cancel()
                }
            }
        }
        job.join()
        assertEquals(listOf("first"),delivered)
        assertEquals(1,socket.cancelled)
    }

    @Test fun cancelledPollCannotReleaseAQueuedChunk() = runBlocking {
        var delivered=false
        val job=launch {
            val owner=coroutineContext.job
            val events=object:LinkedBlockingDeque<GeminiLivePresetEvent>() {
                override fun poll():GeminiLivePresetEvent? {
                    owner.cancel()
                    return GeminiLivePresetEvent.Chunk("late")
                }
            }
            awaitGeminiLivePresetEvent(events,contract.getLong("firstOutputTimeoutMillis"))
            delivered=true
        }
        job.join()
        assertFalse(delivered)
    }

    @Test fun cancellationInterruptsAnEmptyResponseWait() = runBlocking {
        var delivered = false
        val job = launch {
            awaitGeminiLivePresetEvent(LinkedBlockingDeque(), contract.getLong("firstOutputTimeoutMillis"))
            delivered = true
        }
        delay(10)
        withTimeout(500) { job.cancelAndJoin() }
        assertFalse(delivered)
    }

    @Test fun queuedEventsKeepTheirOrder() = runBlocking {
        val events = LinkedBlockingDeque<GeminiLivePresetEvent>()
        events.add(GeminiLivePresetEvent.Chunk("first"))
        events.add(GeminiLivePresetEvent.Complete)
        assertEquals(GeminiLivePresetEvent.Chunk("first"), awaitGeminiLivePresetEvent(events, 100))
        assertEquals(GeminiLivePresetEvent.Complete, awaitGeminiLivePresetEvent(events, 100))
    }
}

private class RecordingSocket:WebSocket {
    var cancelled=0
    override fun request():Request=Request.Builder().url("https://localhost/").build()
    override fun queueSize():Long=0
    override fun send(text:String):Boolean=true
    override fun send(bytes:ByteString):Boolean=true
    override fun close(code:Int,reason:String?):Boolean=true
    override fun cancel(){cancelled++}
}
