package dev.screengoated.toolbox.mobile.shared.live

import java.io.File
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicReference
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.async
import kotlinx.coroutines.yield
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assert.assertFalse
import org.junit.Test

class GeminiLiveActiveBufferTest {
    private fun active(): GeminiLiveSessionCore {
        val core = GeminiLiveSessionCore()
        core.listener.onOpen()
        assertTrue(core.activate())
        return core
    }

    @Test fun limitsMatchSharedFixture() {
        val root = generateSequence(File(requireNotNull(System.getProperty("user.dir"))).absoluteFile) { it.parentFile }
            .first { File(it, FIXTURE).isFile }
        val fixture = Json.parseToJsonElement(File(root, FIXTURE).readText()).jsonObject
        val limits = fixture.getValue("activeBuffer").jsonObject
        assertEquals(MAX_ACTIVE_FRAMES, limits.getValue("maxFrames").jsonPrimitive.int)
        assertEquals(MAX_ACTIVE_BYTES, limits.getValue("maxUtf8Bytes").jsonPrimitive.int)
        assertEquals("terminal_error", limits.getValue("overflow").jsonPrimitive.content)
        assertEquals("false", limits.getValue("drainAcceptedBeforeError").jsonPrimitive.content)
        assertEquals("true", limits.getValue("discardIsExplicit").jsonPrimitive.content)
    }

    @Test fun fullFrameBufferReportsOverflowWithoutDeliveringQueuedCompletion() = runTest {
        val core = active()
        assertEquals(GeminiLiveCoreEvent.Opened, core.receiveEvent())
        core.listener.onText("""{"serverContent":{"turnComplete":true}}""")
        repeat(MAX_ACTIVE_FRAMES - 1) { core.listener.onText("event-$it") }
        core.listener.onText("overflow")
        assertEquals(
            GeminiLiveCoreEvent.Failed(GeminiLiveSessionFailure.Server("Gemini Live receive buffer limit exceeded; buffered events discarded.")),
            core.receiveEvent(),
        )
        assertTrue(core.receiveEvent() is GeminiLiveCoreEvent.Failed)
    }

    @Test fun bytesAreBoundedAndDequeuesReclaimCapacity() = runTest {
        val core = active()
        core.receiveEvent()
        val half = "é".repeat(MAX_ACTIVE_BYTES / 4)
        core.listener.onText(half)
        core.listener.onText(half)
        assertEquals(half, (core.receiveEvent() as GeminiLiveCoreEvent.Unparsed).payload)
        core.listener.onText(half)
        core.listener.onText("overflow")
        assertEquals(
            GeminiLiveCoreEvent.Failed(GeminiLiveSessionFailure.Server("Gemini Live receive buffer limit exceeded; buffered events discarded.")),
            core.receiveEvent(),
        )
    }

    @Test fun closeRemainsObservableWhenPayloadBufferIsFull() = runTest {
        val core = active()
        core.receiveEvent()
        repeat(MAX_ACTIVE_FRAMES) { core.listener.onText("event-$it") }
        core.listener.onClosed(1001, "going away")
        repeat(MAX_ACTIVE_FRAMES) { index ->
            assertEquals("event-$index", (core.receiveEvent() as GeminiLiveCoreEvent.Unparsed).payload)
        }
        assertEquals(GeminiLiveCoreEvent.Closed(1001, "going away"), core.receiveEvent())
    }

    @Test fun overflowAndPendingReplayHaveOneAdmissionBoundary() {
        val core = active()
        runBlocking { core.receiveEvent() }
        repeat(MAX_ACTIVE_FRAMES) { core.listener.onText("event-$it") }
        val entered = CountDownLatch(1)
        val release = CountDownLatch(1)
        val overflowFinished = CountDownLatch(1)
        val replay = AtomicReference<Pair<GeminiLiveSessionFailure?, String?>>()
        val reader = Thread {
            replay.set(core.pollPending { entered.countDown(); release.await(); "completion" })
        }
        val writer = Thread { core.listener.onText("overflow"); overflowFinished.countDown() }
        reader.start()
        try {
            assertTrue(entered.await(2, TimeUnit.SECONDS))
            writer.start()
            assertFalse(overflowFinished.await(100, TimeUnit.MILLISECONDS))
        } finally {
            release.countDown()
            reader.join(2_000)
            if (writer.isAlive) writer.join(2_000)
        }
        assertEquals(null to "completion", replay.get())
        assertEquals(0L, overflowFinished.count)
        var dequeued = false
        val (failure, pending) = core.pollPending { dequeued = true; "completion" }
        assertTrue(failure is GeminiLiveSessionFailure.Server)
        assertEquals(null, pending)
        assertFalse(dequeued)
    }

    @Test fun acceptedFramePrecedesConcurrentTransportFailure() {
        val entered = CountDownLatch(1)
        val release = CountDownLatch(1)
        val terminated = CountDownLatch(1)
        val core = GeminiLiveSessionCore { payload ->
            entered.countDown()
            release.await()
            parseGeminiLiveServerFrame(payload)
        }
        core.listener.onOpen()
        assertTrue(core.activate())
        runBlocking { core.receiveEvent() }
        val reader = Thread { core.listener.onText("""{"serverContent":{"inputTranscription":{"text":"before"}}}""") }
        val closer = Thread { core.fail(GeminiLiveSessionFailure.Server("send failed")); terminated.countDown() }
        reader.start()
        try {
            assertTrue(entered.await(2, TimeUnit.SECONDS))
            closer.start()
            assertFalse(terminated.await(100, TimeUnit.MILLISECONDS))
        } finally {
            release.countDown()
            reader.join(2_000)
            if (closer.isAlive) closer.join(2_000)
        }
        runBlocking {
            assertEquals("before", (core.receiveEvent() as GeminiLiveCoreEvent.Frame).frame.inputTranscript)
            assertEquals(GeminiLiveCoreEvent.Failed(GeminiLiveSessionFailure.Server("send failed")), core.receiveEvent())
        }
    }

    @Test fun cancelledWakeupLeavesTheAcceptedFrameQueued() = runTest {
        val core = active()
        core.receiveEvent()
        val waiting = async(start = CoroutineStart.UNDISPATCHED) { core.receiveEvent() }
        core.listener.onText("accepted")
        waiting.cancel()
        yield()
        assertEquals("accepted", (core.receiveEvent() as GeminiLiveCoreEvent.Unparsed).payload)
    }

    private companion object {
        private const val FIXTURE = "parity-fixtures/gemini-live-session/android-event-buffer.json"
    }
}
