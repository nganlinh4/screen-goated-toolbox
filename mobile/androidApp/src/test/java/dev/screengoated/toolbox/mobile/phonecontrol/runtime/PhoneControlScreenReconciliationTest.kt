package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlEffectCertainty
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlTurnPhase
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveFunctionCall
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecycleEffect
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveServerFrame
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Test
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

class PhoneControlScreenReconciliationTest {
    @Test
    fun `fresh streamed screen releases a generation deferred for reconciliation`() {
        val harness = Harness()
        try {
            harness.completeUncertainAction()
            harness.completeGeneration()
            assertEquals(PhoneControlTurnPhase.WORKING, harness.coordinator.phase)

            harness.coordinator.freshScreenEvidenceDelivered()

            assertEquals(PhoneControlTurnPhase.IDLE, harness.coordinator.phase)
        } finally {
            harness.close()
        }
    }

    @Test
    fun `fresh streamed screen never completes an active generation early`() {
        val harness = Harness()
        try {
            harness.completeUncertainAction()

            harness.coordinator.freshScreenEvidenceDelivered()

            assertEquals(PhoneControlTurnPhase.WORKING, harness.coordinator.phase)
            harness.completeGeneration()
            assertEquals(PhoneControlTurnPhase.IDLE, harness.coordinator.phase)
        } finally {
            harness.close()
        }
    }

    private class Harness {
        private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
        private val executor = DeferredExecutor()
        val coordinator = PhoneControlTurnCoordinator(executor, scope, NoOpSink)

        fun completeUncertainAction() {
            val call = GeminiLiveFunctionCall("uncertain-action", "act", JsonObject(emptyMap()))
            coordinator.userSpeechStarted(assistantPlaybackActive = false)
            coordinator.handleFrame(
                frame = GeminiLiveServerFrame(
                    inputTranscript = "perform an action",
                    toolCalls = listOf(call),
                    toolCallPresent = true,
                ),
                effects = listOf(GeminiLiveLifecycleEffect.DispatchTools(listOf(call.id))),
            )
            executor.complete(
                PhoneControlToolExecutionResult(
                    response = buildJsonObject {
                        put("effect_status", "may_have_occurred")
                        put("effect_may_have_occurred", true)
                    },
                    certainty = PhoneControlEffectCertainty.MAY_HAVE_OCCURRED,
                    refreshScreenFrame = true,
                ),
            )
            coordinator.drainToolCompletions()
        }

        fun completeGeneration() {
            coordinator.handleFrame(
                frame = GeminiLiveServerFrame(generationComplete = true),
                effects = listOf(GeminiLiveLifecycleEffect.FinalizeGeneration),
            )
        }

        fun close() {
            coordinator.stop()
            scope.cancel()
        }
    }

    private class DeferredExecutor : PhoneControlToolExecutor {
        private lateinit var completion: PhoneControlToolCompletion
        private val dispatched = CountDownLatch(1)

        override fun execute(
            request: PhoneControlToolRequest,
            completion: PhoneControlToolCompletion,
        ): PhoneControlToolJob {
            this.completion = completion
            dispatched.countDown()
            return PhoneControlToolJob { PhoneControlEffectCertainty.MAY_HAVE_OCCURRED }
        }

        fun complete(result: PhoneControlToolExecutionResult) {
            check(dispatched.await(5, TimeUnit.SECONDS)) { "Tool request was not dispatched" }
            completion.complete(result)
        }
    }

    private object NoOpSink : PhoneControlTurnSink {
        override fun sendPayload(payload: String): Boolean = true
        override fun playAudio(bytes: ByteArray) = Unit
        override fun interruptPlayback() = Unit
        override fun discardQueuedPlayback() = Unit
        override fun updateInputCaption(text: String) = Unit
        override fun updateOutputCaption(text: String) = Unit
        override fun updateTurnPhase(phase: PhoneControlTurnPhase) = Unit
        override fun reconciliationRequired() = Unit
        override fun requestScreenRefresh() = Unit
    }
}
