package dev.screengoated.toolbox.mobile.phonecontrol

import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlEffectCertainty
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlToolAdmission
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlToolCompletion
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlToolCompletionEvent
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlToolController
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlToolExecutionResult
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlToolExecutor
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlToolJob
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlToolRequest
import dev.screengoated.toolbox.mobile.phonecontrol.tools.PhoneControlToolRegistry
import java.util.concurrent.atomic.AtomicInteger
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PhoneControlDebugProbePolicyTest {
    @Test
    fun mutationAcknowledgementComesOnlyFromTheExactRegistryHandlerContract() {
        PhoneControlToolRegistry.specs.forEach { spec ->
            assertEquals(
                "unexpected unacknowledged decision for ${spec.name}",
                !spec.requiresMutationAcknowledgement,
                debugProbeAllows(spec.name, mutationAcknowledged = false),
            )
            assertTrue(debugProbeAllows(spec.name, mutationAcknowledged = true))
        }
    }

    @Test
    fun unknownFutureNamesRemainDispatchableToTheTypedUnknownToolBoundary() {
        assertTrue(debugProbeAllows("future_provider_tool", mutationAcknowledged = false))
    }

    @Test
    fun admissionKeepsOneJobOwnedUntilThatExactLeaseSettles() {
        val admission = DebugProbeAdmission()
        val cancellations = AtomicInteger()
        val firstOperation = DebugProbeOperation("first").apply {
            attachCancellation { cancellations.incrementAndGet() }
        }
        val secondOperation = DebugProbeOperation("second")
        val first = requireNotNull(admission.tryAdmit("first", firstOperation))

        assertEquals(null, admission.tryAdmit("second", secondOperation))
        admission.cancel("first")
        assertEquals(1, cancellations.get())
        assertEquals(null, admission.tryAdmit("second", secondOperation))

        admission.release(DebugProbeLease("first", DebugProbeOperation("first")))
        assertEquals(null, admission.tryAdmit("second", secondOperation))
        admission.release(first)
        assertTrue(admission.tryAdmit("second", secondOperation) != null)
    }

    @Test
    fun cancellationBeforeToolJobAttachIsDeliveredExactlyOnceAfterAttach() {
        val operation = DebugProbeOperation("cancel-before-attach")
        val cancellations = AtomicInteger()

        operation.requestCancellation(suppressFutureReceipt = true)
        assertEquals(0, cancellations.get())
        operation.attachCancellation { cancellations.incrementAndGet() }
        operation.requestCancellation(suppressFutureReceipt = true)

        assertEquals(1, cancellations.get())
    }

    @Test
    fun cancelledOperationCannotPublishALateReceipt() {
        val operation = DebugProbeOperation("no-resurrection")
        var published = false

        operation.requestCancellation(suppressFutureReceipt = true)

        assertFalse(operation.publishReceiptIfAllowed { published = true })
        assertFalse(published)
    }

    @Test
    fun admissionRemainsOwnedUntilProductionControllerDeliversTerminalCompletion() = runBlocking {
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
        val completions = Channel<PhoneControlToolCompletionEvent>(capacity = 1)
        val completionCallback = CompletableDeferred<PhoneControlToolCompletion>()
        val cancelCalled = CompletableDeferred<Unit>()
        val executor = PhoneControlToolExecutor { _, completion ->
            completionCallback.complete(completion)
            PhoneControlToolJob {
                cancelCalled.complete(Unit)
                PhoneControlEffectCertainty.MAY_HAVE_OCCURRED
            }
        }
        val controller = PhoneControlToolController(executor, scope, completions)
        val admission = DebugProbeAdmission()
        val operation = DebugProbeOperation("terminal-owner")
        val lease = requireNotNull(admission.tryAdmit(operation.requestId, operation))

        try {
            assertEquals(
                PhoneControlToolAdmission.ACCEPTED,
                controller.dispatch(
                    PhoneControlToolRequest(
                        id = operation.requestId,
                        name = "act",
                        arguments = JsonObject(emptyMap()),
                        turnId = 1,
                        generation = 1,
                    ),
                ),
            )
            operation.attachCancellation {
                controller.cancel(listOf(operation.requestId))
            }
            val callback = withTimeout(TEST_TIMEOUT_MS) { completionCallback.await() }

            admission.cancel(operation.requestId)
            withTimeout(TEST_TIMEOUT_MS) { cancelCalled.await() }
            assertEquals(
                null,
                admission.tryAdmit("later", DebugProbeOperation("later")),
            )
            assertEquals(null, withTimeoutOrNull(PROMPT_TIMEOUT_MS) { completions.receive() })

            callback.complete(
                PhoneControlToolExecutionResult(
                    response = buildJsonObject {
                        put("code", "tool_cancelled")
                        put("terminal_cancellation_acknowledged", true)
                    },
                    certainty = PhoneControlEffectCertainty.MAY_HAVE_OCCURRED,
                ),
            )
            val event = withTimeout(TEST_TIMEOUT_MS) { completions.receive() }
            assertTrue(controller.takeCompletion(event) != null)
            admission.release(lease)
            assertTrue(
                admission.tryAdmit("later", DebugProbeOperation("later")) != null,
            )
        } finally {
            admission.release(lease)
            scope.cancel()
        }
    }

    @Test
    fun dispatcherStorePreservesStatefulProvidersForTheDebugProcessLifetime() {
        val creations = AtomicInteger()
        val store = DebugProbeDispatcherStore<Any>()

        val first = store.getOrCreate {
            creations.incrementAndGet()
            Any()
        }
        val second = store.getOrCreate {
            creations.incrementAndGet()
            Any()
        }

        assertTrue(first === second)
        assertEquals(1, creations.get())
    }

    private companion object {
        const val PROMPT_TIMEOUT_MS = 100L
        const val TEST_TIMEOUT_MS = 5_000L
    }
}
