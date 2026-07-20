package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import dev.screengoated.toolbox.mobile.phonecontrol.memory.PhoneControlMemoryRecordInput
import dev.screengoated.toolbox.mobile.phonecontrol.memory.PhoneControlMemoryRepository
import dev.screengoated.toolbox.mobile.phonecontrol.memory.PhoneControlMemoryRole
import dev.screengoated.toolbox.mobile.phonecontrol.memory.PhoneControlMemoryTurnInput
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlEffectCertainty
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlTurnPhase
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecycleEffect
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveServerFrame
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class PhoneControlMemoryRuntimeTest {
    @get:Rule
    val temporaryFolder = TemporaryFolder()

    @Test
    fun `process startup recovers stale drafts exactly once`() {
        val repository = repository()
        repository.append("stale", turnInput("turn", 10L, "request", "response"))
        val startup = PhoneControlMemoryStartup(repository) { 50L }

        assertTrue(startup.recoverOnce())
        assertFalse(startup.recoverOnce())

        val recovered = requireNotNull(repository.get("stale"))
        assertEquals(50L, recovered.finalizedAtEpochMs)
        assertEquals(listOf("request", "response"), recovered.records.map { it.text })
    }

    @Test
    fun `complete pair accepts late user revision and finalizes once`() {
        val repository = repository()
        var now = 100L
        val recorder = PhoneControlMemoryTurnRecorder(
            repository = repository,
            clock = { now++ },
            sessionId = "live-session",
        )
        recorder.turnStarted(turnId = 1L, generation = 1L)
        recorder.userTranscriptUpdated(1L, "partial request")
        recorder.assistantTranscriptUpdated(1L, "complete response")

        recorder.turnCompleted(1L, "partial request", "complete response")
        assertNull(repository.get("live-session"))
        recorder.userTranscriptUpdated(1L, "complete request")
        recorder.finalizeSession()

        val finalized = requireNotNull(repository.get("live-session"))
        assertEquals(
            listOf(PhoneControlMemoryRole.USER, PhoneControlMemoryRole.ASSISTANT),
            finalized.records.map { it.role },
        )
        assertEquals(listOf("complete request", "complete response"), finalized.records.map { it.text })
        val revision = finalized.revision
        recorder.finalizeSession()
        assertEquals(revision, repository.get("live-session")?.revision)
    }

    @Test
    fun `interrupted incomplete turn is absent while earlier complete pair survives`() {
        val repository = repository()
        var now = 200L
        val recorder = PhoneControlMemoryTurnRecorder(
            repository = repository,
            clock = { now++ },
            sessionId = "mixed-session",
        )
        recorder.turnStarted(1L, 1L)
        recorder.turnCompleted(1L, "kept request", "kept response")
        recorder.turnStarted(2L, 2L)
        recorder.userTranscriptUpdated(2L, "discarded request")
        recorder.assistantTranscriptUpdated(2L, "partial response")

        recorder.turnInterrupted(2L)
        recorder.finalizeSession()

        val finalized = requireNotNull(repository.get("mixed-session"))
        assertEquals(listOf("1", "1"), finalized.records.map { it.turnId })
        assertEquals(listOf("kept request", "kept response"), finalized.records.map { it.text })
    }

    @Test
    fun `session with only interrupted work creates no durable sidecar`() {
        val repository = repository()
        val recorder = PhoneControlMemoryTurnRecorder(
            repository = repository,
            clock = { 300L },
            sessionId = "interrupted-only",
        )
        recorder.turnStarted(1L, 1L)
        recorder.userTranscriptUpdated(1L, "unfinished")
        recorder.turnInterrupted(1L)

        recorder.finalizeSession()

        assertTrue(repository.list().isEmpty())
        assertTrue(repository.paths().sessions.listFiles().orEmpty().isEmpty())
    }

    @Test
    fun `coordinator completion and late revision persist as one finalized pair`() {
        val repository = repository()
        val recorder = PhoneControlMemoryTurnRecorder(
            repository = repository,
            clock = { 400L },
            sessionId = "coordinator-session",
        )
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
        val executor = PhoneControlToolExecutor { _, _ ->
            PhoneControlToolJob { PhoneControlEffectCertainty.PROVEN_NO_EFFECT }
        }
        val coordinator = PhoneControlTurnCoordinator(executor, scope, NoOpTurnSink, recorder)
        try {
            coordinator.handleFrame(
                GeminiLiveServerFrame(inputTranscript = "partial request"),
                listOf(GeminiLiveLifecycleEffect.DeliverContent(1)),
            )
            coordinator.handleFrame(
                GeminiLiveServerFrame(outputTranscript = "complete response"),
                listOf(GeminiLiveLifecycleEffect.DeliverContent(1)),
            )
            coordinator.handleFrame(
                GeminiLiveServerFrame(generationComplete = true),
                listOf(GeminiLiveLifecycleEffect.FinalizeGeneration),
            )
            coordinator.handleFrame(
                GeminiLiveServerFrame(inputTranscript = "partial request completed"),
                listOf(GeminiLiveLifecycleEffect.DeliverContent(1)),
            )

            assertEquals(PhoneControlTurnPhase.IDLE, coordinator.phase)
            recorder.finalizeSession()
            val stored = requireNotNull(repository.get("coordinator-session"))
            assertEquals(
                listOf("partial request completed", "complete response"),
                stored.records.map { it.text },
            )
        } finally {
            coordinator.stop()
            scope.cancel()
        }
    }

    private fun repository(): PhoneControlMemoryRepository = PhoneControlMemoryRepository(
        root = temporaryFolder.newFolder(),
    )

    private fun turnInput(
        turnId: String,
        createdAt: Long,
        user: String,
        assistant: String,
    ): PhoneControlMemoryTurnInput = PhoneControlMemoryTurnInput(
        turnId = turnId,
        user = record("$turnId-user", turnId, PhoneControlMemoryRole.USER, user, createdAt),
        assistant = record(
            "$turnId-assistant",
            turnId,
            PhoneControlMemoryRole.ASSISTANT,
            assistant,
            createdAt + 1L,
        ),
    )

    private fun record(
        recordId: String,
        turnId: String,
        role: PhoneControlMemoryRole,
        text: String,
        createdAt: Long,
    ): PhoneControlMemoryRecordInput = PhoneControlMemoryRecordInput(
        recordId = recordId,
        turnId = turnId,
        role = role,
        text = text,
        createdAtEpochMs = createdAt,
    )

    private object NoOpTurnSink : PhoneControlTurnSink {
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
