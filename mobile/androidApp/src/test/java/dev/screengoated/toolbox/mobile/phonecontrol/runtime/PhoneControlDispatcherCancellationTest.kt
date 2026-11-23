package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlEffectCertainty
import dev.screengoated.toolbox.mobile.phonecontrol.tools.PhoneControlToolDispatchBoundary
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.cancel
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.put
import java.util.ArrayDeque
import java.util.concurrent.atomic.AtomicInteger
import kotlin.coroutines.CoroutineContext
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class PhoneControlDispatcherCancellationTest {
    @Test
    fun unexpectedDispatcherThrowablePublishesExactlyOneTerminalResult() = runBlocking {
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
        val completion = CompletableDeferred<PhoneControlToolExecutionResult>()
        val completionCount = AtomicInteger(0)
        val boundary = PhoneControlToolDispatchBoundary { _, _, _ ->
            error("unexpected provider failure")
        }

        try {
            val toolJob = PhoneControlDispatcherToolExecutor(boundary, scope).execute(
                request("unexpected-failure"),
                PhoneControlToolCompletion { result ->
                    completionCount.incrementAndGet()
                    completion.complete(result)
                },
            )
            val terminal = withTimeout(TIMEOUT_MS) { completion.await() }

            assertEquals("tool_execution_failed", terminal.response.getValue("code").jsonPrimitive.content)
            assertEquals(PhoneControlEffectCertainty.UNKNOWN, terminal.certainty)
            assertEquals(PhoneControlEffectCertainty.UNKNOWN, toolJob.cancel())
            assertEquals(1, completionCount.get())
        } finally {
            scope.cancel()
        }
    }

    @Test
    fun cancellationBeforeCoroutineDispatchStillPublishesTerminalNoEffectReceipt() = runBlocking {
        val dispatcher = QueuedDispatcher()
        val scope = CoroutineScope(SupervisorJob() + dispatcher)
        val boundaryCalls = AtomicInteger(0)
        val completion = CompletableDeferred<PhoneControlToolExecutionResult>()
        val boundary = PhoneControlToolDispatchBoundary { _, _, _ ->
            boundaryCalls.incrementAndGet()
            error("cancelled work must never reach dispatch")
        }

        try {
            val toolJob = PhoneControlDispatcherToolExecutor(boundary, scope).execute(
                request("before-coroutine-dispatch"),
                PhoneControlToolCompletion { result -> completion.complete(result) },
            )

            assertEquals(PhoneControlEffectCertainty.PROVEN_NO_EFFECT, toolJob.cancel())
            dispatcher.runAll()
            val terminal = withTimeout(TIMEOUT_MS) { completion.await() }

            assertEquals(0, boundaryCalls.get())
            assertEquals(PhoneControlEffectCertainty.PROVEN_NO_EFFECT, terminal.certainty)
            assertTrue(
                terminal.response.getValue("terminal_cancellation_acknowledged")
                    .jsonPrimitive.boolean,
            )
        } finally {
            scope.cancel()
        }
    }

    @Test
    fun cancellationAfterWorkerStartBeforeEffectBeginGetsTerminalNoEffectReceipt() = runBlocking {
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
        val workerStarted = CompletableDeferred<Unit>()
        val completion = CompletableDeferred<PhoneControlToolExecutionResult>()
        val boundary = PhoneControlToolDispatchBoundary { job, _, _ ->
            workerStarted.complete(Unit)
            try {
                awaitCancellation()
            } finally {
                // The provider reached its owned boundary after cancellation won.
                job.effectOwner.beginEffect()?.close()
            }
        }

        try {
            val toolJob = PhoneControlDispatcherToolExecutor(boundary, scope).execute(
                request("before-effect"),
                PhoneControlToolCompletion { result -> completion.complete(result) },
            )
            withTimeout(TIMEOUT_MS) { workerStarted.await() }

            assertEquals(PhoneControlEffectCertainty.UNKNOWN, toolJob.cancel())
            val terminal = withTimeout(TIMEOUT_MS) { completion.await() }

            assertEquals(PhoneControlEffectCertainty.PROVEN_NO_EFFECT, terminal.certainty)
            assertTrue(
                terminal.response.getValue("terminal_cancellation_acknowledged")
                    .jsonPrimitive.boolean,
            )
        } finally {
            scope.cancel()
        }
    }

    @Test
    fun acceptedEffectCancellationWaitsForPlatformTerminalCallback() = runBlocking {
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
        val accepted = CompletableDeferred<Unit>()
        val platformTerminal = CompletableDeferred<Unit>()
        val completion = CompletableDeferred<PhoneControlToolExecutionResult>()
        val boundary = PhoneControlToolDispatchBoundary { job, _, _ ->
            val lease = requireNotNull(job.effectOwner.beginEffect())
            check(lease.dispatchIfActive { accepted.complete(Unit) })
            try {
                awaitCancellation()
            } finally {
                withContext(NonCancellable) {
                    platformTerminal.await()
                    lease.close()
                }
            }
        }

        try {
            val toolJob = PhoneControlDispatcherToolExecutor(boundary, scope).execute(
                request("accepted-effect"),
                PhoneControlToolCompletion { result -> completion.complete(result) },
            )
            withTimeout(TIMEOUT_MS) { accepted.await() }

            assertEquals(PhoneControlEffectCertainty.MAY_HAVE_OCCURRED, toolJob.cancel())
            assertNull(withTimeoutOrNull(SHORT_WAIT_MS) { completion.await() })

            platformTerminal.complete(Unit)
            val terminal = withTimeout(TIMEOUT_MS) { completion.await() }
            assertEquals(PhoneControlEffectCertainty.MAY_HAVE_OCCURRED, terminal.certainty)
            assertTrue(
                terminal.response.getValue("terminal_cancellation_acknowledged")
                    .jsonPrimitive.boolean,
            )
        } finally {
            scope.cancel()
        }
    }

    @Test
    fun cancellationReceiptWaitsForAndPreservesTerminalRecoveryEvidence() = runBlocking {
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
        val accepted = CompletableDeferred<Unit>()
        val platformTerminal = CompletableDeferred<Unit>()
        val completion = CompletableDeferred<PhoneControlToolExecutionResult>()
        val evidence = buildJsonObject {
            put("kind", "file_transaction")
            put("retained_backup", "/owned/recovery")
        }
        val boundary = PhoneControlToolDispatchBoundary { job, _, _ ->
            val lease = requireNotNull(job.effectOwner.beginEffect())
            lease.markAccepted()
            accepted.complete(Unit)
            try {
                awaitCancellation()
            } finally {
                withContext(NonCancellable) {
                    platformTerminal.await()
                    lease.recordTerminalEvidence(evidence)
                    lease.close()
                }
            }
        }
        try {
            val job = PhoneControlDispatcherToolExecutor(boundary, scope).execute(
                request("retained-recovery"), PhoneControlToolCompletion { completion.complete(it) },
            )
            withTimeout(TIMEOUT_MS) { accepted.await() }
            assertEquals(PhoneControlEffectCertainty.MAY_HAVE_OCCURRED, job.cancel())
            assertNull(withTimeoutOrNull(SHORT_WAIT_MS) { completion.await() })
            platformTerminal.complete(Unit)
            val result = withTimeout(TIMEOUT_MS) { completion.await() }
            assertEquals(PhoneControlEffectCertainty.MAY_HAVE_OCCURRED, result.certainty)
            assertEquals(evidence, result.response.getValue("terminal_effect_evidence").jsonArray.single())
        } finally {
            scope.cancel()
        }
    }

    private fun request(id: String) = PhoneControlToolRequest(
        id = id,
        name = "act",
        arguments = JsonObject(emptyMap()),
        turnId = 13,
        generation = 17,
    )

    private class QueuedDispatcher : CoroutineDispatcher() {
        private val queue = ArrayDeque<Runnable>()

        override fun dispatch(context: CoroutineContext, block: Runnable) {
            synchronized(queue) { queue.addLast(block) }
        }

        fun runAll() {
            while (true) {
                val next = synchronized(queue) { queue.pollFirst() } ?: return
                next.run()
            }
        }
    }

    private companion object {
        const val TIMEOUT_MS = 5_000L
        const val SHORT_WAIT_MS = 100L
    }
}
