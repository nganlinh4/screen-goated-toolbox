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
