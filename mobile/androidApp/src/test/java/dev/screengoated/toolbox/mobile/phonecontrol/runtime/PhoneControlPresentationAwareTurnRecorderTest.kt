package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class PhoneControlPresentationAwareTurnRecorderTest {
    @Test
    fun `silent internal turn never reaches conversation memory`() {
        var suppressed = true
        val delegate = RecordingRecorder()
        val recorder = PhoneControlPresentationAwareTurnRecorder(delegate) { suppressed }

        recorder.turnStarted(1, 1)
        suppressed = false
        recorder.userTranscriptUpdated(1, "internal setup goal")
        recorder.assistantTranscriptUpdated(1, "private output")
        recorder.turnCompleted(1, "internal setup goal", "private output")

        suppressed = true
        recorder.turnStarted(2, 2)
        suppressed = false
        recorder.turnInterrupted(2)

        assertTrue(delegate.events.isEmpty())

        recorder.turnStarted(3, 3)
        recorder.userTranscriptUpdated(3, "user request")
        recorder.assistantTranscriptUpdated(3, "answer")
        recorder.turnCompleted(3, "user request", "answer")

        assertEquals(
            listOf("start:3", "user:user request", "assistant:answer", "complete:3"),
            delegate.events,
        )
    }

    private class RecordingRecorder : PhoneControlTurnRecorder {
        val events = mutableListOf<String>()

        override fun turnStarted(turnId: Long, generation: Long) {
            events += "start:$turnId"
        }

        override fun userTranscriptUpdated(turnId: Long, text: String) {
            events += "user:$text"
        }

        override fun assistantTranscriptUpdated(turnId: Long, text: String) {
            events += "assistant:$text"
        }

        override fun turnCompleted(turnId: Long, userText: String, assistantText: String) {
            events += "complete:$turnId"
        }

        override fun turnInterrupted(turnId: Long) {
            events += "interrupt:$turnId"
        }
    }
}
