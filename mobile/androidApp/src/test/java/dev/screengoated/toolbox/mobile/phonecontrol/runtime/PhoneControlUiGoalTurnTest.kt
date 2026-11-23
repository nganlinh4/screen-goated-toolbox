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
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class PhoneControlUiGoalTurnTest {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    private val sink = Sink()
    private val started = mutableListOf<Long>()
    private val assistants = mutableListOf<Pair<Long, String>>()
    private val coordinator = PhoneControlTurnCoordinator(
        executor = { _, _ -> PhoneControlToolJob { PhoneControlEffectCertainty.PROVEN_NO_EFFECT } },
        scope = scope,
        sink = sink,
        recorder = Recorder(),
    )

    @Test
    fun uiGoalAfterFirstTurn_opensNewGeneration() {
        try {
            frame(GeminiLiveServerFrame(inputTranscript = "first request"), GeminiLiveLifecycleEffect.DeliverContent(1))
            frame(GeminiLiveServerFrame(outputTranscript = "first answer"), GeminiLiveLifecycleEffect.DeliverContent(1))
            frame(GeminiLiveServerFrame(generationComplete = true), GeminiLiveLifecycleEffect.FinalizeGeneration)
            assertEquals(PhoneControlTurnPhase.IDLE, coordinator.phase)

            coordinator.userInterfaceGoalSent()
            assertEquals(listOf(1L, 2L), started)
            assertEquals(PhoneControlTurnPhase.WORKING, coordinator.phase)

            frame(GeminiLiveServerFrame(outputTranscript = "second answer"), GeminiLiveLifecycleEffect.DeliverContent(1))
            val call = GeminiLiveFunctionCall("goal-observe", "observe", JsonObject(emptyMap()))
            frame(
                GeminiLiveServerFrame(toolCalls = listOf(call), toolCallPresent = true),
                GeminiLiveLifecycleEffect.DispatchTools(listOf(call.id)),
            )

            assertEquals(2L to "second answer", assistants.last())
            assertEquals(1, coordinator.pendingWorkCount)
            assertTrue(sink.payloads.none { "turn_closed" in it })
        } finally {
            coordinator.stop()
            scope.cancel()
        }
    }

    @Test
    fun uiGoalDuringActiveGeneration_keepsTheTurn() {
        try {
            frame(GeminiLiveServerFrame(inputTranscript = "request"), GeminiLiveLifecycleEffect.DeliverContent(1))
            coordinator.userInterfaceGoalSent()
            assertEquals(listOf(1L), started)
        } finally {
            coordinator.stop()
            scope.cancel()
        }
    }

    private fun frame(frame: GeminiLiveServerFrame, effect: GeminiLiveLifecycleEffect) {
        coordinator.handleFrame(frame, listOf(effect))
    }

    private inner class Recorder : PhoneControlTurnRecorder {
        override fun turnStarted(turnId: Long, generation: Long) {
            started += turnId
        }
        override fun userTranscriptUpdated(turnId: Long, text: String) = Unit
        override fun assistantTranscriptUpdated(turnId: Long, text: String) {
            assistants += turnId to text
        }
        override fun turnCompleted(turnId: Long, userText: String, assistantText: String) = Unit
        override fun turnInterrupted(turnId: Long) = Unit
    }

    private class Sink : PhoneControlTurnSink {
        val payloads = mutableListOf<String>()
        override fun sendPayload(payload: String): Boolean = payloads.add(payload)
        override fun playAudio(bytes: ByteArray) = Unit
        override fun interruptPlayback() = Unit
        override fun discardQueuedPlayback() = Unit
        override fun updateInputCaption(text: String) = Unit
        override fun updateOutputCaption(text: String) = Unit
        override fun updateTurnPhase(phase: PhoneControlTurnPhase) = Unit
        override fun requestScreenRefresh() = Unit
    }
}
