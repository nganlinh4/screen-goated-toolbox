package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlEffectCertainty
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlTurnPhase
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveFunctionCall
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecycleEffect
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveContentPart
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveServerFrame
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.boolean
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
    fun `local speech bursts keep active input until provider interruption`() {
        val harness = Harness()
        try {
            harness.input("old request")

            harness.coordinator.userSpeechActivity(assistantPlaybackActive = true, burstStarted = true)
            harness.input("old request revised")
            assertEquals(1, harness.recorder.started.size)
            assertTrue(harness.recorder.interrupted.isEmpty())

            harness.coordinator.userSpeechActivity(assistantPlaybackActive = false, burstStarted = true)
            harness.input("old request revised with details")
            assertEquals(1, harness.recorder.started.size)
            harness.interrupt()
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

            harness.coordinator.userSpeechActivity(assistantPlaybackActive = true, burstStarted = true)
            harness.input("second request")

            assertEquals(listOf(1L, 2L), harness.recorder.started.map(Started::turnId))
            assertEquals(UserUpdate(2L, "second request"), harness.recorder.users.last())
            harness.coordinator.userSpeechActivity(assistantPlaybackActive = false, burstStarted = true)
            harness.input("second request revised")
            assertEquals(2, harness.recorder.started.size)
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

    @Test
    fun `silent presentation discards assistant content without disabling tools`() {
        val harness = Harness(surfaceAssistantContent = false)
        try {
            harness.input("internal setup")
            val call = GeminiLiveFunctionCall("observe-setup", "observe", JsonObject(emptyMap()))
            harness.coordinator.handleFrame(
                GeminiLiveServerFrame(
                    outputTranscript = "private generated output",
                    toolCalls = listOf(call),
                    toolCallPresent = true,
                ),
                listOf(
                    GeminiLiveLifecycleEffect.DeliverContent(1),
                    GeminiLiveLifecycleEffect.DispatchTools(listOf(call.id)),
                ),
            )

            assertTrue(harness.sink.outputs.none { "private generated output" in it })
            assertTrue(harness.recorder.assistants.isEmpty())
            assertTrue(harness.sink.orbPresentations.isEmpty())
            assertTrue(harness.sink.phases.isEmpty())
            assertEquals(1, harness.coordinator.pendingWorkCount)
        } finally {
            harness.close()
        }
    }

    @Test
    fun `audio mode model text stays internal`() {
        val harness = Harness()
        try {
            harness.input("request")
            val outputCount = harness.sink.outputs.size
            val orbCount = harness.sink.orbPresentations.size
            val phaseCount = harness.sink.phases.size

            harness.coordinator.handleFrame(
                GeminiLiveServerFrame(
                    contentParts = listOf(
                        GeminiLiveContentPart(
                            text = "internal tool narration",
                            thought = false,
                            inlineData = null,
                        ),
                    ),
                ),
                listOf(GeminiLiveLifecycleEffect.DeliverContent(1)),
            )

            assertEquals(outputCount, harness.sink.outputs.size)
            assertEquals(orbCount, harness.sink.orbPresentations.size)
            assertEquals(phaseCount, harness.sink.phases.size)
            assertTrue(harness.recorder.assistants.isEmpty())
        } finally {
            harness.close()
        }
    }

    @Test
    fun `shared speech input epoch replays match Windows`() {
        val fixture = generateSequence(java.io.File(requireNotNull(System.getProperty("user.dir")))) { it.parentFile }
            .map { java.io.File(it, "parity-fixtures/phone-control/speech-input-epochs.json") }
            .first { it.isFile }
        val cases = Json.parseToJsonElement(fixture.readText()).jsonObject.getValue("cases").jsonArray
        for (case in cases) {
            val value = case.jsonObject
            val harness = Harness()
            try {
                for (step in value.getValue("steps").jsonArray) {
                    val item = step.jsonObject
                    when (val event = item.getValue("event").jsonPrimitive.content) {
                        "input" -> harness.input(item.getValue("text").jsonPrimitive.content)
                        "output" -> harness.output(item.getValue("text").jsonPrimitive.content)
                        "complete" -> harness.completeGeneration()
                        "interrupt" -> harness.interrupt()
                        "speech_start", "speech_continues" -> harness.coordinator.userSpeechActivity(
                            item.getValue("playing").jsonPrimitive.boolean, burstStarted = event == "speech_start",
                        )
                        else -> error("Unknown speech replay event: $event")
                    }
                }
                val expected = value.getValue("userTurns").jsonArray.map { it.jsonPrimitive.content }
                val actual = harness.recorder.users.associate { it.turnId to it.text }.values.toList()
                assertEquals(value.getValue("name").jsonPrimitive.content, expected, actual)
                assertEquals(expected.size, harness.recorder.started.size)
                assertEquals(value.getValue("active").jsonPrimitive.boolean, harness.coordinator.phase == PhoneControlTurnPhase.WORKING)
            } finally { harness.close() }
        }
    }

    @Test
    fun `protocol rebound preserves active transcript and rearms retired input`() {
        val harness = Harness()
        try {
            harness.input("first request")
            harness.coordinator.freshProtocolSessionBound()
            harness.input("with details")
            assertEquals(1, harness.recorder.started.size)
            assertEquals(UserUpdate(1L, "first request with details"), harness.recorder.users.last())
            harness.coordinator.abandonProtocolSession()
            harness.coordinator.freshProtocolSessionBound()
            harness.input("new request")
            assertEquals(listOf(1L, 2L), harness.recorder.started.map(Started::turnId))
            assertEquals(UserUpdate(2L, "new request"), harness.recorder.users.last())
        } finally { harness.close() }
    }

    @Test
    fun `provider cancellation arms input only for the owned job`() {
        val harness = Harness()
        try {
            harness.input("old request")
            val call = GeminiLiveFunctionCall("owned-job", "observe", JsonObject(emptyMap()))
            harness.coordinator.handleFrame(GeminiLiveServerFrame(toolCalls = listOf(call), toolCallPresent = true),
                listOf(GeminiLiveLifecycleEffect.DispatchTools(listOf(call.id))))
            val interrupts = harness.sink.playbackInterrupts
            harness.coordinator.handleFrame(GeminiLiveServerFrame(),
                listOf(GeminiLiveLifecycleEffect.CancelTools(listOf("retired-job"))))
            assertEquals(interrupts, harness.sink.playbackInterrupts)
            harness.input("old request revised")
            assertEquals(1, harness.recorder.started.size)
            harness.coordinator.handleFrame(GeminiLiveServerFrame(),
                listOf(GeminiLiveLifecycleEffect.CancelTools(listOf(call.id))))
            assertEquals(interrupts + 1, harness.sink.playbackInterrupts)
            harness.input("new request")
            assertEquals(listOf(1L, 2L), harness.recorder.started.map(Started::turnId))
            assertEquals(UserUpdate(2L, "new request"), harness.recorder.users.last())
            assertEquals(1, harness.coordinator.pendingWorkCount)
        } finally { harness.close() }
    }

    private class Harness(surfaceAssistantContent: Boolean = true) {
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
        val executor = CountingExecutor()
        val sink = RecordingSink(surfaceAssistantContent)
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

    private class RecordingSink(
        private val surfaceAssistantContent: Boolean,
    ) : PhoneControlTurnSink {
        var playbackInterrupts = 0
        val payloads = mutableListOf<String>()
        val inputs = mutableListOf<String>()
        val outputs = mutableListOf<String>()
        val orbPresentations = mutableListOf<Pair<String, String?>>()
        val phases = mutableListOf<PhoneControlTurnPhase>()

        override fun sendPayload(payload: String): Boolean = payloads.add(payload)
        override fun playAudio(bytes: ByteArray) = Unit
        override fun interruptPlayback() { playbackInterrupts += 1 }
        override fun discardQueuedPlayback() = Unit
        override fun updateInputCaption(text: String) {
            inputs += text
        }
        override fun updateOutputCaption(text: String) {
            outputs += text
        }
        override fun surfaceAssistantContent(): Boolean = surfaceAssistantContent
        override fun updateOrbPresentation(stateLabel: String, iconOverride: String?) {
            orbPresentations += stateLabel to iconOverride
        }
        override fun updateTurnPhase(phase: PhoneControlTurnPhase) {
            phases += phase
        }
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
