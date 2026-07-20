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

class PhoneControlTurnCoordinatorTranscriptTest {
    @Test
    fun `coalesced interruption assigns input to a new epoch before other effects`() {
        val harness = Harness()
        try {
            harness.input("old request")
            val staleCall = GeminiLiveFunctionCall("stale", "observe", JsonObject(emptyMap()))

            harness.coordinator.handleFrame(
                frame = GeminiLiveServerFrame(
                    inputTranscript = "new request",
                    outputTranscript = "late old output",
                    interrupted = true,
                    toolCalls = listOf(staleCall),
                    toolCallPresent = true,
                ),
                effects = listOf(
                    GeminiLiveLifecycleEffect.DeliverContent(2),
                    GeminiLiveLifecycleEffect.DispatchTools(listOf(staleCall.id)),
                    GeminiLiveLifecycleEffect.StopPlayback,
                    GeminiLiveLifecycleEffect.DiscardQueuedOutput,
                    GeminiLiveLifecycleEffect.FinalizeInterruptedGeneration,
                ),
            )

            assertEquals(listOf(1L, 2L), harness.recorder.started.map(Started::turnId))
            assertEquals(listOf(1L), harness.recorder.interrupted)
            assertEquals(UserUpdate(2L, "new request"), harness.recorder.users.last())
            assertTrue(harness.recorder.assistants.isEmpty())
            assertEquals(0, harness.executor.executions)
            assertTrue(harness.sink.payloads.single().contains("turn_closed"))
            assertEquals(PhoneControlTurnPhase.WORKING, harness.coordinator.phase)
        } finally {
            harness.close()
        }
    }

    @Test
    fun `interruption without transcript creates no durable phantom turn`() {
        val harness = Harness()
        try {
            harness.input("old request")
            harness.interrupt()

            assertEquals(listOf(1L), harness.recorder.started.map(Started::turnId))
            assertEquals(listOf(1L), harness.recorder.interrupted)
            assertEquals(PhoneControlTurnPhase.LISTENING, harness.coordinator.phase)

            harness.input("request after interruption")

            assertEquals(listOf(1L, 2L), harness.recorder.started.map(Started::turnId))
            assertEquals(PhoneControlTurnPhase.WORKING, harness.coordinator.phase)
        } finally {
            harness.close()
        }
    }

    @Test
    fun `local speech onset replaces active work only when playback is quiet`() {
        val harness = Harness()
        try {
            harness.input("old request")

            harness.coordinator.userSpeechStarted(assistantPlaybackActive = true)
            harness.input("old request revised")
            assertEquals(1, harness.recorder.started.size)
            assertTrue(harness.recorder.interrupted.isEmpty())

            harness.coordinator.userSpeechStarted(assistantPlaybackActive = false)
            harness.input("new request")

            assertEquals(listOf(1L, 2L), harness.recorder.started.map(Started::turnId))
            assertEquals(listOf(1L), harness.recorder.interrupted)
            assertEquals(UserUpdate(2L, "new request"), harness.recorder.users.last())
        } finally {
            harness.close()
        }
    }

    @Test
    fun `late transcript revises caption but cannot reopen a completed turn`() {
        val harness = Harness()
        try {
            harness.input("first request")
            harness.output("answer")
            harness.completeGeneration()

            harness.input("first request revised")

            assertEquals(1, harness.recorder.started.size)
            assertEquals(1, harness.recorder.completed.size)
            assertEquals(UserUpdate(1L, "first request revised"), harness.recorder.users.last())
            assertEquals("first request revised", harness.sink.inputs.last())
            assertEquals(PhoneControlTurnPhase.IDLE, harness.coordinator.phase)

            harness.coordinator.userSpeechStarted(assistantPlaybackActive = true)
            harness.input("first request revised again")
            assertEquals(1, harness.recorder.started.size)

            harness.coordinator.userSpeechStarted(assistantPlaybackActive = false)
            harness.input("second request")

            assertEquals(listOf(1L, 2L), harness.recorder.started.map(Started::turnId))
            assertEquals(UserUpdate(2L, "second request"), harness.recorder.users.last())
        } finally {
            harness.close()
        }
    }

    @Test
    fun `assistant fragments form one pair and duplicate completion is absorbed`() {
        val harness = Harness()
        try {
            harness.input("question")
            harness.output("answer begins")
            harness.output("answer begins here")
            harness.output("here now")
            harness.output("here now")
            harness.completeGeneration()
            harness.completeGeneration()

            assertEquals("answer begins here now", harness.sink.outputs.last())
            assertEquals(
                listOf(Completed(1L, "question", "answer begins here now")),
                harness.recorder.completed,
            )
        } finally {
            harness.close()
        }
    }

    private class Harness {
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
        val executor = CountingExecutor()
        val sink = RecordingSink()
        val recorder = RecordingRecorder()
        val coordinator = PhoneControlTurnCoordinator(executor, scope, sink, recorder)

        fun input(text: String) {
            coordinator.handleFrame(
                GeminiLiveServerFrame(inputTranscript = text),
                listOf(GeminiLiveLifecycleEffect.DeliverContent(1)),
            )
        }

        fun output(text: String) {
            coordinator.handleFrame(
                GeminiLiveServerFrame(outputTranscript = text),
                listOf(GeminiLiveLifecycleEffect.DeliverContent(1)),
            )
        }

        fun interrupt() {
            coordinator.handleFrame(
                GeminiLiveServerFrame(interrupted = true),
                listOf(
                    GeminiLiveLifecycleEffect.StopPlayback,
                    GeminiLiveLifecycleEffect.DiscardQueuedOutput,
                    GeminiLiveLifecycleEffect.FinalizeInterruptedGeneration,
                ),
            )
        }

        fun completeGeneration() {
            coordinator.handleFrame(
                GeminiLiveServerFrame(generationComplete = true),
                listOf(GeminiLiveLifecycleEffect.FinalizeGeneration),
            )
        }

        fun close() {
            coordinator.stop()
            scope.cancel()
        }
    }

    private class CountingExecutor : PhoneControlToolExecutor {
        var executions = 0

        override fun execute(
            request: PhoneControlToolRequest,
            completion: PhoneControlToolCompletion,
        ): PhoneControlToolJob {
            executions += 1
            return PhoneControlToolJob { PhoneControlEffectCertainty.PROVEN_NO_EFFECT }
        }
    }

    private class RecordingSink : PhoneControlTurnSink {
        val payloads = mutableListOf<String>()
        val inputs = mutableListOf<String>()
        val outputs = mutableListOf<String>()

        override fun sendPayload(payload: String): Boolean = payloads.add(payload)
        override fun playAudio(bytes: ByteArray) = Unit
        override fun interruptPlayback() = Unit
        override fun discardQueuedPlayback() = Unit
        override fun updateInputCaption(text: String) {
            inputs += text
        }
        override fun updateOutputCaption(text: String) {
            outputs += text
        }
        override fun updateTurnPhase(phase: PhoneControlTurnPhase) = Unit
        override fun reconciliationRequired() = Unit
        override fun requestScreenRefresh() = Unit
    }

    private class RecordingRecorder : PhoneControlTurnRecorder {
        val started = mutableListOf<Started>()
        val users = mutableListOf<UserUpdate>()
        val assistants = mutableListOf<UserUpdate>()
        val completed = mutableListOf<Completed>()
        val interrupted = mutableListOf<Long>()

        override fun turnStarted(turnId: Long, generation: Long) {
            started += Started(turnId, generation)
        }
        override fun userTranscriptUpdated(turnId: Long, text: String) {
            users += UserUpdate(turnId, text)
        }
        override fun assistantTranscriptUpdated(turnId: Long, text: String) {
            assistants += UserUpdate(turnId, text)
        }
        override fun turnCompleted(turnId: Long, userText: String, assistantText: String) {
            completed += Completed(turnId, userText, assistantText)
        }
        override fun turnInterrupted(turnId: Long) {
            interrupted += turnId
        }
    }

    private data class Started(val turnId: Long, val generation: Long)
    private data class UserUpdate(val turnId: Long, val text: String)
    private data class Completed(val turnId: Long, val user: String, val assistant: String)
}
