package dev.screengoated.toolbox.mobile.preset

import java.util.concurrent.LinkedBlockingDeque
import java.io.File
import org.json.JSONObject
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

class GeminiLivePresetWaitTest {
    private val contract = JSONObject(generateSequence(File(requireNotNull(System.getProperty("user.dir")))) { it.parentFile }
        .map { File(it, "parity-fixtures/preset-system/live-response-wait.json") }.first(File::isFile).readText())

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
