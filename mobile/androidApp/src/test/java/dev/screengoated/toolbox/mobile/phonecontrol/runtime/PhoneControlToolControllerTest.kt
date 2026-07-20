package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlEffectCertainty
import java.util.concurrent.ConcurrentLinkedQueue
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executor
import java.util.concurrent.atomic.AtomicInteger
import kotlin.coroutines.CoroutineContext
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.channels.Channel
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test

class PhoneControlToolControllerTest {
    @Test
    fun `cancel before worker claim never dispatches provider and drains local terminal ack`() {
        val dispatcher = PausedDispatcher()
        val executor = ControlledExecutor()
        val completions = Channel<PhoneControlToolCompletionEvent>(capacity = 1)
        val controller = PhoneControlToolController(
            executor,
            CoroutineScope(SupervisorJob() + dispatcher),
            completions,
        )

        assertEquals(PhoneControlToolAdmission.ACCEPTED, controller.dispatch(request("owner")))
        assertEquals(
            listOf(PhoneControlEffectCertainty.PROVEN_NO_EFFECT),
            controller.cancelAll(),
        )
        assertEquals(1, controller.pendingCount)

        val event = completions.tryReceive().getOrNull()
        assertNotNull(event)
        assertNotNull(controller.takeCompletion(requireNotNull(event)))
        dispatcher.runAll()

        assertEquals(0, executor.executions.get())
        assertEquals(0, controller.pendingCount)
    }

    @Test
    fun `started cancellation owns slot until executor terminal completion`() {
        val dispatcher = PausedDispatcher()
        val executor = ControlledExecutor()
        val completions = Channel<PhoneControlToolCompletionEvent>(capacity = 1)
        val controller = PhoneControlToolController(
            executor,
            CoroutineScope(SupervisorJob() + dispatcher),
            completions,
        )

        assertEquals(PhoneControlToolAdmission.ACCEPTED, controller.dispatch(request("owner")))
        dispatcher.runAll()
        assertEquals(1, executor.executions.get())

        assertEquals(
            listOf(PhoneControlEffectCertainty.PROVEN_NO_EFFECT),
            controller.cancelAll(),
        )
        assertEquals(1, controller.pendingCount)
        assertEquals(
            PhoneControlToolAdmission.TOOL_CALL_IN_FLIGHT,
            controller.dispatch(request("later")),
        )

        executor.complete(PhoneControlEffectCertainty.MAY_HAVE_OCCURRED)
        val event = completions.tryReceive().getOrNull()
        assertNotNull(controller.takeCompletion(requireNotNull(event)))
        assertEquals(0, controller.pendingCount)
    }

    @Test
    fun `concurrent admission race produces exactly one owner`() {
        val dispatcher = PausedDispatcher()
        val controller = PhoneControlToolController(
            ControlledExecutor(),
            CoroutineScope(SupervisorJob() + dispatcher),
            Channel(Channel.UNLIMITED),
        )
        val ready = CountDownLatch(2)
        val go = CountDownLatch(1)
        val results = ConcurrentLinkedQueue<PhoneControlToolAdmission>()
        val threads = listOf("first", "second").map { id ->
            Thread {
                ready.countDown()
                go.await()
                results += controller.dispatch(request(id))
            }.also(Thread::start)
        }
        ready.await()
        go.countDown()
        threads.forEach(Thread::join)

        assertEquals(1, results.count { it == PhoneControlToolAdmission.ACCEPTED })
        assertEquals(1, results.count { it == PhoneControlToolAdmission.TOOL_CALL_IN_FLIGHT })
        assertEquals(1, controller.pendingCount)
        assertTrue(controller.cancelAll().isNotEmpty())
    }

    @Test
    fun `broken executor duplicate callbacks produce one terminal event`() {
        val dispatcher = PausedDispatcher()
        val executor = ControlledExecutor()
        val completions = Channel<PhoneControlToolCompletionEvent>(capacity = 1)
        val controller = PhoneControlToolController(
            executor,
            CoroutineScope(SupervisorJob() + dispatcher),
            completions,
        )
        assertEquals(PhoneControlToolAdmission.ACCEPTED, controller.dispatch(request("owner")))
        dispatcher.runAll()

        executor.complete(PhoneControlEffectCertainty.PROVEN_NO_EFFECT)
        executor.complete(PhoneControlEffectCertainty.MAY_HAVE_OCCURRED)

        val event = requireNotNull(completions.tryReceive().getOrNull())
        assertEquals(PhoneControlEffectCertainty.PROVEN_NO_EFFECT, event.result.certainty)
        assertNotNull(controller.takeCompletion(event))
        assertTrue(completions.tryReceive().isFailure)
        assertEquals(0, controller.pendingCount)
    }

    private fun request(id: String) = PhoneControlToolRequest(
        id = id,
        name = "observe",
        arguments = JsonObject(emptyMap()),
        turnId = 1,
        generation = 1,
    )

    private class PausedDispatcher : CoroutineDispatcher(), Executor {
        private val tasks = ConcurrentLinkedQueue<Runnable>()

        override fun dispatch(context: CoroutineContext, block: Runnable) {
            tasks += block
        }

        override fun execute(command: Runnable) {
            tasks += command
        }

        fun runAll() {
            while (true) tasks.poll()?.run() ?: return
        }
    }

    private class ControlledExecutor : PhoneControlToolExecutor {
        val executions = AtomicInteger(0)
        private var completion: PhoneControlToolCompletion? = null

        override fun execute(
            request: PhoneControlToolRequest,
            completion: PhoneControlToolCompletion,
        ): PhoneControlToolJob {
            executions.incrementAndGet()
            this.completion = completion
            return PhoneControlToolJob { PhoneControlEffectCertainty.PROVEN_NO_EFFECT }
        }

        fun complete(certainty: PhoneControlEffectCertainty) {
            requireNotNull(completion).complete(
                PhoneControlToolExecutionResult(
                    response = buildJsonObject { put("code", "tool_cancelled") },
                    certainty = certainty,
                ),
            )
        }
    }
}
