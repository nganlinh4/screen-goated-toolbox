package dev.screengoated.toolbox.mobile.phonecontrol

import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlEffectCertainty
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlTurnPhase
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlToolCompletion
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlToolExecutionResult
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlToolExecutor
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlToolJob
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlToolRequest
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlToolFramePreflight
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlTurnCoordinator
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlTurnSink
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveFunctionCall
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecycleEffect
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveServerFrame
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PhoneControlSingleFlightRuntimeTest {
    @Test
    fun `duplicate owner id is absorbed without a second function response`() {
        fixture().use { test ->
            test.dispatch(
                beginTurn = true,
                call("same-id", "observe"),
                call("same-id", "observe"),
            )
            test.executor.awaitRequest("same-id")
            assertEquals(0, test.coordinator.heldRejectionCount)

            test.executor.complete("same-id", result())
            test.coordinator.drainToolCompletions()

            assertEquals(1, test.sink.payloads.count { "\"id\":\"same-id\"" in it })
        }
    }

    @Test
    fun `rejection flood stays bounded and aborts only after owner terminal`() {
        fixture().use { test ->
            val flood = Array(32) { index -> call("later-$index", "observe") }
            test.dispatch(true, call("flood-owner", "observe"), *flood)
            test.dispatch(false, call("later-overflow", "observe"))
            test.executor.awaitRequest("flood-owner")

            assertEquals(32, test.coordinator.heldRejectionCount)
            assertEquals(0, test.sink.protocolAborts)
            assertTrue(flood.none { test.executor.hasRequest(it.id) })

            test.executor.complete("flood-owner", result())
            test.coordinator.drainToolCompletions()

            assertEquals(listOf("payload:flood-owner", "abort"), test.sink.protocolEvents)
            assertEquals(0, test.coordinator.heldRejectionCount)
            assertEquals(PhoneControlTurnPhase.LISTENING, test.coordinator.phase)

            test.coordinator.abandonProtocolSession()
            test.coordinator.freshProtocolSessionBound()
            test.dispatch(true, call("fresh-owner", "observe"))
            assertEquals("fresh-owner", test.executor.awaitRequest("fresh-owner").id)
        }
    }

    @Test
    fun `overflow suppresses done and a cancelling owner still settles before abort`() {
        fixture(PhoneControlEffectCertainty.MAY_HAVE_OCCURRED).use { test ->
            val flood = Array(32) { index -> call("overflow-$index", "observe") }
            test.dispatch(true, call("overflow-done", "done"), *flood)
            test.dispatch(false, call("overflow-extra", "observe"))
            test.executor.awaitRequest("overflow-done")
            test.executor.complete("overflow-done", result(terminalSummary = "Finished"))
            test.coordinator.drainToolCompletions()

            assertEquals(1, test.sink.protocolAborts)
            assertTrue(test.sink.payloads.none { "\"id\":\"overflow-done\"" in it })

            fixture(PhoneControlEffectCertainty.MAY_HAVE_OCCURRED).use { cancelling ->
                cancelling.dispatch(true, call("cancel-owner", "act"), *flood)
                cancelling.dispatch(false, call("cancel-overflow", "observe"))
                cancelling.executor.awaitRequest("cancel-owner")
                cancelling.dispatch(true, call("barge-call", "observe"))

                assertEquals(1, cancelling.coordinator.pendingWorkCount)
                assertEquals(0, cancelling.sink.protocolAborts)
                cancelling.coordinator.drainToolCompletions()
                assertEquals(1, cancelling.sink.protocolAborts)
                assertEquals(1, cancelling.sink.reconciliationRequests)
                assertFalse(cancelling.executor.hasRequest("barge-call"))
            }
        }
    }

    @Test
    fun `oversized no-owner frame aborts before dispatch or response production`() {
        fixture().use { test ->
            val calls = Array(PhoneControlToolFramePreflight.MAXIMUM_CALLS + 1) { index ->
                call("large-$index", "observe")
            }

            test.dispatch(true, *calls)

            assertTrue(calls.none { test.executor.hasRequest(it.id) })
            assertEquals(0, test.sink.payloads.size)
            assertEquals(1, test.sink.protocolAborts)
            assertEquals(PhoneControlTurnPhase.LISTENING, test.coordinator.phase)
        }
    }

    @Test
    fun `sink refusal stops production until a fresh protocol session is bound`() {
        fixture().use { test ->
            test.sink.acceptPayloads = false
            test.dispatch(true, call("", ""), call("", ""))

            assertEquals(1, test.sink.payloadAttempts)
            assertEquals(1, test.sink.protocolAborts)

            test.coordinator.abandonProtocolSession()
            test.coordinator.freshProtocolSessionBound()
            test.sink.acceptPayloads = true
            test.dispatch(true, call("", ""))

            assertEquals(2, test.sink.payloadAttempts)
            assertEquals(1, test.sink.payloads.size)
        }
    }

    private fun fixture(
        cancellationCertainty: PhoneControlEffectCertainty =
            PhoneControlEffectCertainty.PROVEN_NO_EFFECT,
    ): Fixture = Fixture(cancellationCertainty)

    private class Fixture(cancellationCertainty: PhoneControlEffectCertainty) : AutoCloseable {
        private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
        val executor = RecordingExecutor(cancellationCertainty)
        val sink = RecordingSink()
        val coordinator = PhoneControlTurnCoordinator(executor, scope, sink)

        fun dispatch(
            beginTurn: Boolean,
            vararg calls: GeminiLiveFunctionCall,
        ) {
            if (beginTurn) coordinator.userSpeechStarted(assistantPlaybackActive = false)
            coordinator.handleFrame(
                frame = GeminiLiveServerFrame(
                    inputTranscript = "fixture request".takeIf { beginTurn },
                    toolCalls = calls.toList(),
                    toolCallPresent = true,
                ),
                effects = listOf(
                    GeminiLiveLifecycleEffect.DispatchTools(
                        calls.map(GeminiLiveFunctionCall::id),
                    ),
                ),
            )
        }

        override fun close() {
            coordinator.stop()
            scope.cancel()
        }
    }

    private class RecordingExecutor(
        private val cancellationCertainty: PhoneControlEffectCertainty,
    ) : PhoneControlToolExecutor {
        private data class Entry(
            val request: PhoneControlToolRequest,
            val complete: (PhoneControlToolExecutionResult) -> Unit,
        )

        private val entries = ConcurrentHashMap<String, Entry>()
        private val requests = LinkedBlockingQueue<PhoneControlToolRequest>()

        override fun execute(
            request: PhoneControlToolRequest,
            completion: PhoneControlToolCompletion,
        ): PhoneControlToolJob {
            val terminal = AtomicBoolean(false)
            val completeOnce = { result: PhoneControlToolExecutionResult ->
                if (terminal.compareAndSet(false, true)) completion.complete(result)
            }
            entries[request.id] = Entry(request, completeOnce)
            requests.offer(request)
            return PhoneControlToolJob {
                completeOnce(result(certainty = cancellationCertainty))
                cancellationCertainty
            }
        }

        fun awaitRequest(id: String): PhoneControlToolRequest {
            entries[id]?.let { return it.request }
            val deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(5)
            while (true) {
                val remaining = deadline - System.nanoTime()
                check(remaining > 0L) { "Tool request was not dispatched: $id" }
                val request = requests.poll(remaining, TimeUnit.NANOSECONDS)
                    ?: error("Tool request was not dispatched: $id")
                if (request.id == id) return request
                entries[id]?.let { return it.request }
            }
        }

        fun complete(id: String, result: PhoneControlToolExecutionResult) {
            requireNotNull(entries[id]) { "Missing tool request: $id" }.complete(result)
        }

        fun hasRequest(id: String): Boolean = entries.containsKey(id)
    }

    private class RecordingSink : PhoneControlTurnSink {
        val payloads = mutableListOf<String>()
        val protocolEvents = mutableListOf<String>()
        var protocolAborts = 0
        var reconciliationRequests = 0
        var payloadAttempts = 0
        var acceptPayloads = true

        override fun sendPayload(payload: String): Boolean {
            payloadAttempts += 1
            if (!acceptPayloads) return false
            payloads += payload
            val id = ID_PATTERN.find(payload)?.groupValues?.get(1).orEmpty()
            protocolEvents += "payload:$id"
            return true
        }

        override fun playAudio(bytes: ByteArray) = Unit
        override fun interruptPlayback() = Unit
        override fun discardQueuedPlayback() = Unit
        override fun updateInputCaption(text: String) = Unit
        override fun updateOutputCaption(text: String) = Unit
        override fun updateTurnPhase(phase: PhoneControlTurnPhase) = Unit
        override fun reconciliationRequired() {
            reconciliationRequests += 1
        }
        override fun requestScreenRefresh() = Unit
        override fun abortProtocolSession() {
            protocolAborts += 1
            protocolEvents += "abort"
        }
    }

    private companion object {
        val ID_PATTERN = Regex("\\\"id\\\":\\\"([^\\\"]+)\\\"")

        fun call(id: String, name: String) =
            GeminiLiveFunctionCall(id, name, JsonObject(emptyMap()))

        fun result(
            terminalSummary: String? = null,
            certainty: PhoneControlEffectCertainty = PhoneControlEffectCertainty.PROVEN_NO_EFFECT,
        ) = PhoneControlToolExecutionResult(
            response = buildJsonObject { put("code", "ok") },
            certainty = certainty,
            terminalSummary = terminalSummary,
        )
    }
}
