package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class PhoneControlSessionTurnRecorderTest {
    private class Recorder : PhoneControlTurnRecorder {
        val events = mutableListOf<String>()
        override fun turnStarted(turnId: Long, generation: Long) { events += "start:$turnId" }
        override fun userTranscriptUpdated(turnId: Long, text: String) { events += "user:$turnId:$text" }
        override fun assistantTranscriptUpdated(turnId: Long, text: String) { events += "assistant:$turnId:$text" }
        override fun turnCompleted(turnId: Long, userText: String, assistantText: String) { events += "completed:$turnId" }
        override fun turnInterrupted(turnId: Long) { events += "interrupted:$turnId" }
    }

    @Test fun suppressedTurnsKeepRequestAuthorityAndRetirementWithoutPersistingConversation() {
        var suppressed = true
        val persistent = Recorder()
        val authority = Recorder()
        val recorder = phoneControlSessionTurnRecorder(persistent, listOf(authority)) { suppressed }
        recorder.turnStarted(1, 1)
        recorder.userTranscriptUpdated(1, "requested work")
        recorder.assistantTranscriptUpdated(1, "result")
        suppressed = false
        recorder.turnCompleted(1, "requested work", "result")
        assertTrue(persistent.events.isEmpty())
        assertEquals(listOf("start:1", "user:1:requested work", "assistant:1:result", "completed:1"), authority.events)
        recorder.turnStarted(2, 2)
        recorder.userTranscriptUpdated(2, "visible request")
        recorder.turnInterrupted(2)
        assertEquals(listOf("start:2", "user:2:visible request", "interrupted:2"), persistent.events)
        assertEquals("interrupted:2", authority.events.last())
    }

    @Test fun suppressionAtRetirementStillReleasesSessionAuthority() {
        var suppressed = false
        val persistent = Recorder()
        val authority = Recorder()
        val recorder = phoneControlSessionTurnRecorder(persistent, listOf(authority)) { suppressed }
        recorder.turnStarted(1, 1)
        suppressed = true
        recorder.turnInterrupted(1)
        assertEquals(listOf("start:1", "interrupted:1"), authority.events)
        assertEquals(listOf("start:1"), persistent.events)
    }
}
