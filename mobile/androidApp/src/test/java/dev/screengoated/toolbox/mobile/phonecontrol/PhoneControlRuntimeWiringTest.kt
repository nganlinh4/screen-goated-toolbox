package dev.screengoated.toolbox.mobile.phonecontrol

import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlEffectCertainty
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlTurnPhase
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlDispatcherToolExecutor
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlToolCompletion
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlToolExecutionResult
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlToolExecutor
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlToolJob
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlToolRequest
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlTurnCoordinator
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlTurnSink
import dev.screengoated.toolbox.mobile.phonecontrol.tools.PhoneControlToolDispatchBoundary
import dev.screengoated.toolbox.mobile.phonecontrol.tools.PhoneControlToolExecution
import dev.screengoated.toolbox.mobile.phonecontrol.tools.PhoneControlToolJobContext
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveFunctionCall
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecycleEffect
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveServerFrame
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.cancel
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PhoneControlRuntimeWiringTest {
    @Test
    fun `dispatcher adapter preserves job identity and typed effect status`() = runBlocking {
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
        val captured = CompletableDeferred<BoundaryCall>()
        val completed = CompletableDeferred<PhoneControlToolExecutionResult>()
        val arguments = buildJsonObject { put("target", 7) }
        val response = buildJsonObject {
            put("code", "ok")
            put("effect_status", "verified")
        }
        val boundary = PhoneControlToolDispatchBoundary { job, name, args ->
            captured.complete(BoundaryCall(job, name, args))
            PhoneControlToolExecution(
                response = response,
                mutating = true,
                terminalSummary = "kept",
                refreshScreenFrame = true,
                screenFramePayload = "screen-evidence",
            )
        }
        val request = PhoneControlToolRequest(
            id = "job-exact",
            name = "act",
            arguments = arguments,
            turnId = 17,
            generation = 23,
        )

        try {
            val job = PhoneControlDispatcherToolExecutor(boundary, scope).execute(
                request,
                PhoneControlToolCompletion { result -> completed.complete(result) },
            )
            val call = withTimeout(TIMEOUT_MS) { captured.await() }
            val result = withTimeout(TIMEOUT_MS) { completed.await() }

            assertEquals(17L, call.job.turnId)
            assertEquals("job-exact", call.job.jobId)
            assertEquals(23L, call.job.responseGeneration)
            assertEquals("act", call.name)
            assertEquals(arguments, call.arguments)
            assertEquals(response, result.response)
            assertEquals(PhoneControlEffectCertainty.VERIFIED, result.certainty)
            assertEquals("kept", result.terminalSummary)
            assertTrue(result.refreshScreenFrame)
            assertEquals("screen-evidence", result.screenFramePayload)
            assertEquals(PhoneControlEffectCertainty.VERIFIED, job.cancel())
        } finally {
            scope.cancel()
        }
    }

    @Test
    fun `function receipt is sent before its tool-owned screen evidence`() {
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
        val executor = RecordingExecutor()
        val sink = RecordingSink()
        val coordinator = PhoneControlTurnCoordinator(executor, scope, sink)

        try {
            dispatch(coordinator, beginTurn = true, call("look-frame", "look"))
            executor.awaitRequest("look-frame")
            executor.complete(
                "look-frame",
                toolResult(screenFramePayload = "visual-evidence-payload"),
            )
            coordinator.drainToolCompletions()

            assertEquals(2, sink.payloads.size)
            assertTrue("\"id\":\"look-frame\"" in sink.payloads[0])
            assertEquals("visual-evidence-payload", sink.payloads[1])
        } finally {
            coordinator.stop()
            scope.cancel()
        }
    }

    @Test
    fun `semantic detector gestures are mutating before dispatch completes`() = runBlocking {
        listOf("click_target", "drag_target").forEach { tool ->
            val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
            val started = CompletableDeferred<Unit>()
            val completion = CompletableDeferred<PhoneControlToolExecutionResult>()
            val boundary = PhoneControlToolDispatchBoundary { _, _, _ ->
                started.complete(Unit)
                awaitCancellation()
            }

            try {
                val job = PhoneControlDispatcherToolExecutor(boundary, scope).execute(
                    request(id = "mutating-$tool", name = tool),
                    PhoneControlToolCompletion(completion::complete),
                )
                withTimeout(TIMEOUT_MS) { started.await() }

                assertEquals(
                    "$tool must reconcile an interrupted dispatch",
                    PhoneControlEffectCertainty.UNKNOWN,
                    job.cancel(),
                )
                assertEquals(
                    PhoneControlEffectCertainty.UNKNOWN,
                    withTimeout(TIMEOUT_MS) { completion.await() }.certainty,
                )
            } finally {
                scope.cancel()
            }
        }
    }

    @Test
    fun `done without a validated terminal summary leaves the turn open`() {
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
        val executor = RecordingExecutor()
        val sink = RecordingSink()
        val coordinator = PhoneControlTurnCoordinator(executor, scope, sink)

        try {
            dispatch(coordinator, beginTurn = true, call("done-unvalidated", "done"))
            val done = executor.awaitRequest("done-unvalidated")
            executor.complete("done-unvalidated", toolResult(terminalSummary = null))
            coordinator.drainToolCompletions()

            assertEquals(PhoneControlTurnPhase.WORKING, coordinator.phase)
            dispatch(coordinator, beginTurn = false, call("after-unvalidated", "observe"))
            val followUp = executor.awaitRequest("after-unvalidated")
            assertEquals(done.turnId, followUp.turnId)
            assertEquals(done.generation, followUp.generation)
            assertTrue(sink.payloads.any { payload -> "\"id\":\"done-unvalidated\"" in payload })
        } finally {
            coordinator.stop()
            scope.cancel()
        }
    }

    @Test
    fun `validated done stays terminal while later calls are rejected behind it`() {
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
        val executor = RecordingExecutor()
        val sink = RecordingSink()
        val coordinator = PhoneControlTurnCoordinator(executor, scope, sink)

        try {
            dispatch(
                coordinator,
                beginTurn = true,
                call("done-validated", "done"),
                call("late-observe", "observe"),
            )
            executor.awaitRequest("done-validated")
            assertFalse(executor.hasRequest("late-observe"))
            assertTrue(sink.payloads.isEmpty())

            executor.complete("done-validated", toolResult(terminalSummary = "Finished"))
            coordinator.drainToolCompletions()

            assertEquals(PhoneControlTurnPhase.FINALIZING, coordinator.phase)
            assertEquals(2, sink.payloads.size)
            assertTrue("\"id\":\"late-observe\"" in sink.payloads[0])
            assertTrue("tool_call_in_flight" in sink.payloads[0])
            assertTrue("\"id\":\"done-validated\"" in sink.payloads[1])
        } finally {
            coordinator.stop()
            scope.cancel()
        }
    }

    @Test
    fun `same frame calls execute one at a time and preserve receipt order`() {
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
        val executor = RecordingExecutor()
        val sink = RecordingSink()
        val coordinator = PhoneControlTurnCoordinator(executor, scope, sink)

        try {
            dispatch(
                coordinator,
                beginTurn = true,
                call("owner-observe", "observe"),
                call("later-observe", "observe"),
            )
            executor.awaitRequest("owner-observe")
            assertFalse(executor.hasRequest("later-observe"))
            assertEquals(1, coordinator.pendingWorkCount)
            assertTrue(sink.payloads.isEmpty())

            executor.complete("owner-observe", toolResult())
            coordinator.drainToolCompletions()

            assertEquals(0, coordinator.pendingWorkCount)
            assertEquals(2, sink.payloads.size)
            assertTrue("\"id\":\"owner-observe\"" in sink.payloads[0])
            assertTrue("\"id\":\"later-observe\"" in sink.payloads[1])
            assertTrue("tool_call_in_flight" in sink.payloads[1])
        } finally {
            coordinator.stop()
            scope.cancel()
        }
    }

    @Test
    fun `barge in keeps cancelling work in flight until its terminal acknowledgement`() {
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
        val executor = RecordingExecutor(PhoneControlEffectCertainty.MAY_HAVE_OCCURRED)
        val sink = RecordingSink()
        val coordinator = PhoneControlTurnCoordinator(executor, scope, sink)

        try {
            dispatch(coordinator, beginTurn = true, call("old-action", "act"))
            executor.awaitRequest("old-action")

            dispatch(coordinator, beginTurn = true, call("new-observe", "observe"))

            assertEquals(1, coordinator.pendingWorkCount)
            assertFalse(executor.hasRequest("new-observe"))
            assertEquals(0, sink.reconciliationRequests)
            assertTrue(sink.payloads.isEmpty())

            coordinator.drainToolCompletions()

            assertEquals(0, coordinator.pendingWorkCount)
            assertEquals(1, sink.reconciliationRequests)
            assertEquals(1, sink.payloads.size)
            assertTrue("\"id\":\"new-observe\"" in sink.payloads.single())
            assertTrue(
                sink.payloads.single(),
                "prior_action_settling" in sink.payloads.single(),
            )
        } finally {
            coordinator.stop()
            scope.cancel()
        }
    }

    @Test
    fun `generation completion cannot publish idle while a tool job is pending`() {
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
        val executor = RecordingExecutor()
        val sink = RecordingSink()
        val coordinator = PhoneControlTurnCoordinator(executor, scope, sink)

        try {
            dispatch(coordinator, beginTurn = true, call("late-mutation", "act"))
            executor.awaitRequest("late-mutation")

            coordinator.handleFrame(
                frame = GeminiLiveServerFrame(generationComplete = true),
                effects = listOf(GeminiLiveLifecycleEffect.FinalizeGeneration),
            )
            assertEquals(PhoneControlTurnPhase.WORKING, coordinator.phase)

            executor.complete(
                "late-mutation",
                toolResult(certainty = PhoneControlEffectCertainty.VERIFIED),
            )
            coordinator.drainToolCompletions()
            assertEquals(PhoneControlTurnPhase.WORKING, coordinator.phase)
            assertTrue(sink.payloads.any { "\"id\":\"late-mutation\"" in it })

            coordinator.handleFrame(
                frame = GeminiLiveServerFrame(generationComplete = true),
                effects = listOf(GeminiLiveLifecycleEffect.FinalizeGeneration),
            )
            assertEquals(PhoneControlTurnPhase.IDLE, coordinator.phase)
        } finally {
            coordinator.stop()
            scope.cancel()
        }
    }

    @Test
    fun `uncertain receipt blocks done and later mutations until a fresh observation`() {
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
        val executor = RecordingExecutor()
        val sink = RecordingSink()
        val coordinator = PhoneControlTurnCoordinator(executor, scope, sink)

        try {
            dispatch(coordinator, beginTurn = true, call("uncertain-mutation", "act"))
            executor.awaitRequest("uncertain-mutation")
            executor.complete(
                "uncertain-mutation",
                toolResult(certainty = PhoneControlEffectCertainty.MAY_HAVE_OCCURRED),
            )
            coordinator.drainToolCompletions()

            dispatch(coordinator, beginTurn = false, call("done-pending", "done"))

            assertEquals(PhoneControlTurnPhase.WORKING, coordinator.phase)
            assertEquals(1, sink.reconciliationRequests)
            assertTrue(sink.payloads.any { "blocked_reconciliation_required" in it })

            dispatch(coordinator, beginTurn = false, call("blocked-mutation", "act"))
            assertFalse(executor.hasRequest("blocked-mutation"))
            assertTrue(sink.payloads.any { "\"id\":\"blocked-mutation\"" in it })

            dispatch(coordinator, beginTurn = false, call("fresh-observe", "observe"))
            executor.awaitRequest("fresh-observe")
            executor.complete("fresh-observe", toolResult(stateReconciled = true))
            coordinator.drainToolCompletions()
            dispatch(coordinator, beginTurn = false, call("after-observe", "act"))
            assertEquals("after-observe", executor.awaitRequest("after-observe").id)
        } finally {
            coordinator.stop()
            scope.cancel()
        }
    }

    @Test
    fun `fresh postcondition in an uncertain receipt never publishes a transient warning`() {
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
        val executor = RecordingExecutor()
        val sink = RecordingSink()
        val coordinator = PhoneControlTurnCoordinator(executor, scope, sink)

        try {
            dispatch(coordinator, beginTurn = true, call("verified-by-read", "act"))
            executor.awaitRequest("verified-by-read")
            executor.complete(
                "verified-by-read",
                toolResult(
                    certainty = PhoneControlEffectCertainty.MAY_HAVE_OCCURRED,
                    stateReconciled = true,
                ),
            )
            coordinator.drainToolCompletions()

            assertEquals(0, sink.reconciliationRequests)
            dispatch(coordinator, beginTurn = false, call("next-action", "act"))
            assertEquals("next-action", executor.awaitRequest("next-action").id)
        } finally {
            coordinator.stop()
            scope.cancel()
        }
    }

    @Test
    fun `a newly admitted user turn retires playback before dispatch`() {
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
        val executor = RecordingExecutor()
        val sink = RecordingSink()
        val coordinator = PhoneControlTurnCoordinator(executor, scope, sink)

        try {
            dispatch(coordinator, beginTurn = true, call("first", "observe"))
            executor.awaitRequest("first")
            assertEquals(1, sink.playbackInterrupts)
            assertEquals(1, sink.playbackDiscards)

            executor.complete("first", toolResult())
            coordinator.drainToolCompletions()
            coordinator.handleFrame(
                frame = GeminiLiveServerFrame(generationComplete = true),
                effects = listOf(GeminiLiveLifecycleEffect.FinalizeGeneration),
            )

            dispatch(coordinator, beginTurn = true, call("second", "observe"))
            executor.awaitRequest("second")
            assertEquals(2, sink.playbackInterrupts)
            assertEquals(2, sink.playbackDiscards)
        } finally {
            coordinator.stop()
            scope.cancel()
        }
    }

    private fun dispatch(
        coordinator: PhoneControlTurnCoordinator,
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
                GeminiLiveLifecycleEffect.DispatchTools(calls.map(GeminiLiveFunctionCall::id)),
            ),
        )
    }

    private fun request(id: String, name: String): PhoneControlToolRequest {
        return PhoneControlToolRequest(
            id = id,
            name = name,
            arguments = JsonObject(emptyMap()),
            turnId = 1,
            generation = 1,
        )
    }

    private fun call(id: String, name: String): GeminiLiveFunctionCall {
        return GeminiLiveFunctionCall(id, name, JsonObject(emptyMap()))
    }

    private fun toolResult(
        terminalSummary: String? = null,
        screenFramePayload: String? = null,
        certainty: PhoneControlEffectCertainty = PhoneControlEffectCertainty.PROVEN_NO_EFFECT,
        stateReconciled: Boolean = false,
    ): PhoneControlToolExecutionResult = PhoneControlToolExecutionResult(
        response = buildJsonObject {
            put("code", "ok")
            put(
                "effect_status",
                when (certainty) {
                    PhoneControlEffectCertainty.VERIFIED -> "verified"
                    PhoneControlEffectCertainty.MAY_HAVE_OCCURRED -> "may_have_occurred"
                    PhoneControlEffectCertainty.PROVEN_NO_EFFECT -> "proven_no_effect"
                    PhoneControlEffectCertainty.UNKNOWN -> "unknown"
                },
            )
            if (stateReconciled) put("state_reconciled", true)
        },
        certainty = certainty,
        terminalSummary = terminalSummary,
        screenFramePayload = screenFramePayload,
    )

    private data class BoundaryCall(
        val job: PhoneControlToolJobContext,
        val name: String,
        val arguments: JsonObject,
    )

    private class RecordingExecutor(
        private val cancellationCertainty: PhoneControlEffectCertainty =
            PhoneControlEffectCertainty.PROVEN_NO_EFFECT,
    ) : PhoneControlToolExecutor {
        private data class Entry(
            val request: PhoneControlToolRequest,
            val complete: (PhoneControlToolExecutionResult) -> Unit,
            val job: RecordingJob,
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
            val job = RecordingJob(cancellationCertainty) {
                completeOnce(
                    PhoneControlToolExecutionResult(
                        response = buildJsonObject { put("code", "tool_cancelled") },
                        certainty = cancellationCertainty,
                    ),
                )
            }
            entries[request.id] = Entry(request, completeOnce, job)
            requests.offer(request)
            return job
        }

        fun awaitRequest(id: String): PhoneControlToolRequest {
            entries[id]?.let { return it.request }
            val deadline = System.nanoTime() + TimeUnit.MILLISECONDS.toNanos(TIMEOUT_MS)
            while (true) {
                val remaining = deadline - System.nanoTime()
                if (remaining <= 0L) error("Tool request was not dispatched: $id")
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

    private class RecordingJob(
        private val cancellationCertainty: PhoneControlEffectCertainty,
        private val acknowledge: () -> Unit,
    ) : PhoneControlToolJob {
        override fun cancel(): PhoneControlEffectCertainty {
            acknowledge()
            return cancellationCertainty
        }
    }

    private class RecordingSink : PhoneControlTurnSink {
        val payloads = mutableListOf<String>()
        var playbackInterrupts = 0
        var playbackDiscards = 0
        var reconciliationRequests = 0

        override fun sendPayload(payload: String): Boolean = payloads.add(payload)
        override fun playAudio(bytes: ByteArray) = Unit
        override fun interruptPlayback() {
            playbackInterrupts += 1
        }
        override fun discardQueuedPlayback() {
            playbackDiscards += 1
        }
        override fun updateInputCaption(text: String) = Unit
        override fun updateOutputCaption(text: String) = Unit
        override fun updateTurnPhase(phase: PhoneControlTurnPhase) = Unit
        override fun reconciliationRequired() {
            reconciliationRequests += 1
        }
        override fun requestScreenRefresh() = Unit
    }

    private companion object {
        private const val TIMEOUT_MS = 5_000L
    }
}
