package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlTurnPhase
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class PhoneControlOutboundDiagnosticsTest {
    @Test
    fun `pending function call pauses ambient screen but not microphone audio`() {
        assertTrue(canSendAmbientScreen(0))
        assertFalse(canSendAmbientScreen(1))
    }

    @Test
    fun `UI goal waits for an idle quiet single flight boundary`() {
        assertTrue(
            canSendUserInterfaceGoal(
                phase = PhoneControlTurnPhase.LISTENING,
                pendingWorkCount = 0,
                userSpeaking = false,
                goalInFlight = false,
            ),
        )
        assertTrue(
            canSendUserInterfaceGoal(
                phase = PhoneControlTurnPhase.IDLE,
                pendingWorkCount = 0,
                userSpeaking = false,
                goalInFlight = false,
            ),
        )
        assertFalse(
            canSendUserInterfaceGoal(
                phase = PhoneControlTurnPhase.WORKING,
                pendingWorkCount = 0,
                userSpeaking = false,
                goalInFlight = false,
            ),
        )
        assertFalse(
            canSendUserInterfaceGoal(
                phase = PhoneControlTurnPhase.LISTENING,
                pendingWorkCount = 1,
                userSpeaking = false,
                goalInFlight = false,
            ),
        )
        assertFalse(
            canSendUserInterfaceGoal(
                phase = PhoneControlTurnPhase.LISTENING,
                pendingWorkCount = 0,
                userSpeaking = true,
                goalInFlight = false,
            ),
        )
        assertFalse(
            canSendUserInterfaceGoal(
                phase = PhoneControlTurnPhase.LISTENING,
                pendingWorkCount = 0,
                userSpeaking = false,
                goalInFlight = true,
            ),
        )
    }

    @Test
    fun `UI goal queue keeps only the latest explicit UI action`() {
        val queue = PhoneControlUserInterfaceGoalQueue(maximumChars = 64)
        assertEquals(
            PhoneControlUiGoalOffer.QUEUED,
            queue.offer("first goal", runtimeReady = true).disposition,
        )
        val latest = queue.offer("latest goal", runtimeReady = true)
        assertEquals(PhoneControlUiGoalOffer.REPLACED, latest.disposition)
        var payload = ""
        assertEquals(
            PhoneControlUiGoalFlush.SENT,
            queue.flush(
                phase = PhoneControlTurnPhase.LISTENING,
                pendingWorkCount = 0,
                userSpeaking = false,
                send = {
                    payload = it
                    true
                },
            ),
        )
        assertTrue("latest goal" in payload)
        assertFalse("first goal" in payload)
        assertEquals(
            PhoneControlUiGoalFlush.WAITING,
            queue.flush(
                phase = PhoneControlTurnPhase.LISTENING,
                pendingWorkCount = 0,
                userSpeaking = false,
                send = { true },
            ),
        )
        assertNull(queue.observeTurnBoundary(interrupted = false))
        assertNull(
            queue.settle(
                phase = PhoneControlTurnPhase.WORKING,
                pendingWorkCount = 0,
                playbackDrained = true,
            ),
        )
        assertNull(
            queue.settle(
                phase = PhoneControlTurnPhase.LISTENING,
                pendingWorkCount = 0,
                playbackDrained = false,
            ),
        )
        assertEquals(
            PhoneControlUiGoalCompletion(
                requireNotNull(latest.id),
                PhoneControlUiGoalOutcome.COMPLETED,
            ),
            queue.settle(
                phase = PhoneControlTurnPhase.IDLE,
                pendingWorkCount = 0,
                playbackDrained = true,
            ),
        )
        assertEquals(
            PhoneControlUiGoalFlush.NONE,
            queue.flush(
                phase = PhoneControlTurnPhase.LISTENING,
                pendingWorkCount = 0,
                userSpeaking = false,
                send = { true },
            ),
        )
    }

    @Test
    fun `UI goal completion stays correlated across queued replacement and interruption`() {
        val queue = PhoneControlUserInterfaceGoalQueue(maximumChars = 64)
        val first = queue.offer("first", runtimeReady = true)
        assertEquals(
            PhoneControlUiGoalFlush.SENT,
            queue.flush(
                phase = PhoneControlTurnPhase.LISTENING,
                pendingWorkCount = 0,
                userSpeaking = false,
                send = { true },
            ),
        )
        val second = queue.offer("second", runtimeReady = true)
        assertNull(queue.observeTurnBoundary(interrupted = false))
        assertEquals(
            first.id,
            queue.settle(
                phase = PhoneControlTurnPhase.IDLE,
                pendingWorkCount = 0,
                playbackDrained = true,
            )?.id,
        )
        assertEquals(
            PhoneControlUiGoalFlush.SENT,
            queue.flush(
                phase = PhoneControlTurnPhase.LISTENING,
                pendingWorkCount = 0,
                userSpeaking = false,
                send = { true },
            ),
        )
        assertEquals(
            PhoneControlUiGoalCompletion(
                requireNotNull(second.id),
                PhoneControlUiGoalOutcome.INTERRUPTED,
            ),
            queue.observeTurnBoundary(interrupted = true),
        )
    }

    @Test
    fun `silent UI goal suppresses conversation only while it owns the turn`() {
        val queue = PhoneControlUserInterfaceGoalQueue(maximumChars = 64)
        val goal = queue.offer(
            "silent setup",
            runtimeReady = true,
            presentation = PhoneControlUiGoalPresentation.SILENT,
        )
        assertFalse(queue.conversationSurfaceSuppressed)
        var suppressedDuringSend = false
        assertEquals(
            PhoneControlUiGoalFlush.SENT,
            queue.flush(
                phase = PhoneControlTurnPhase.IDLE,
                pendingWorkCount = 0,
                userSpeaking = false,
                send = {
                    suppressedDuringSend = queue.conversationSurfaceSuppressed
                    true
                },
            ),
        )
        assertTrue(suppressedDuringSend)
        assertTrue(queue.conversationSurfaceSuppressed)
        assertNull(queue.observeTurnBoundary(interrupted = false))
        assertTrue(queue.conversationSurfaceSuppressed)
        assertEquals(
            goal.id,
            queue.settle(
                phase = PhoneControlTurnPhase.IDLE,
                pendingWorkCount = 0,
                playbackDrained = true,
            )?.id,
        )
        assertFalse(queue.conversationSurfaceSuppressed)
    }

    @Test
    fun `protected checkpoint retires only its exact queued UI goal`() {
        val queue = PhoneControlUserInterfaceGoalQueue()
        val offered = queue.offer(
            "navigate to protected checkpoint",
            runtimeReady = true,
            presentation = PhoneControlUiGoalPresentation.SILENT,
        )
        val goalId = requireNotNull(offered.id)

        assertNull(queue.retireForProtectedCheckpoint(goalId + 1))
        assertEquals(
            PhoneControlUiGoalCompletion(goalId, PhoneControlUiGoalOutcome.PROTECTED_CHECKPOINT),
            queue.retireForProtectedCheckpoint(goalId),
        )
        assertFalse(queue.conversationSurfaceSuppressed)
    }

    @Test
    fun `protected checkpoint retires an in flight silent UI goal`() {
        val queue = PhoneControlUserInterfaceGoalQueue()
        val goalId = requireNotNull(
            queue.offer(
                "navigate to protected checkpoint",
                runtimeReady = true,
                presentation = PhoneControlUiGoalPresentation.SILENT,
            ).id,
        )
        assertEquals(
            PhoneControlUiGoalFlush.SENT,
            queue.flush(
                phase = PhoneControlTurnPhase.IDLE,
                pendingWorkCount = 0,
                userSpeaking = false,
                send = { true },
            ),
        )
        assertTrue(queue.conversationSurfaceSuppressed)

        assertEquals(
            PhoneControlUiGoalCompletion(goalId, PhoneControlUiGoalOutcome.PROTECTED_CHECKPOINT),
            queue.retireForProtectedCheckpoint(goalId),
        )
        assertFalse(queue.conversationSurfaceSuppressed)
        assertNull(
            queue.settle(
                phase = PhoneControlTurnPhase.IDLE,
                pendingWorkCount = 0,
                playbackDrained = true,
            ),
        )
    }

    @Test
    fun `rejected silent goal send rolls back conversation suppression`() {
        val queue = PhoneControlUserInterfaceGoalQueue(maximumChars = 64)
        queue.offer(
            "silent setup",
            runtimeReady = true,
            presentation = PhoneControlUiGoalPresentation.SILENT,
        )
        var suppressedDuringSend = false

        assertEquals(
            PhoneControlUiGoalFlush.REJECTED,
            queue.flush(
                phase = PhoneControlTurnPhase.IDLE,
                pendingWorkCount = 0,
                userSpeaking = false,
                send = {
                    suppressedDuringSend = queue.conversationSurfaceSuppressed
                    false
                },
            ),
        )

        assertTrue(suppressedDuringSend)
        assertFalse(queue.conversationSurfaceSuppressed)
        assertEquals(
            PhoneControlUiGoalFlush.SENT,
            queue.flush(
                phase = PhoneControlTurnPhase.IDLE,
                pendingWorkCount = 0,
                userSpeaking = false,
                send = { true },
            ),
        )
        assertTrue(queue.conversationSurfaceSuppressed)
    }

    @Test
    fun `failure tail is bounded structural metadata without payload content`() {
        var now = 100L
        val diagnostics = PhoneControlOutboundDiagnostics { now }
        repeat(PhoneControlOutboundDiagnostics.MAXIMUM_RECORDS + 2) { index ->
            diagnostics.record(
                kind = if (index % 2 == 0) {
                    PhoneControlOutboundKind.TOOL_RESPONSE
                } else {
                    PhoneControlOutboundKind.MICROPHONE_AUDIO
                },
                utf8Bytes = index + 1,
                pendingWork = index % 2,
                turnPhase = PhoneControlTurnPhase.WORKING,
                accepted = true,
            )
            now += 10L
        }

        val tail = diagnostics.describe()
        assertFalse("oldest bounded record must be evicted", "tool_response:1:" in tail)
        assertTrue("microphone_audio:8:" in tail)
        assertFalse("diagnostics never receive or expose content", "secret" in tail)
    }
}
