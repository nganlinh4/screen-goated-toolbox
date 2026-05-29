package dev.screengoated.toolbox.mobile.service

import dev.screengoated.toolbox.mobile.shared.live.LiveSessionState
import dev.screengoated.toolbox.mobile.shared.live.LiveTextState
import org.junit.Assert.assertEquals
import org.junit.Test

class OverlayControllerWebViewStateTest {
    @Test
    fun `transcript split keeps frozen restart prefix visible`() {
        val state = LiveSessionState(
            liveText = LiveTextState(
                frozenPrefix = "old session.",
                fullTranscript = "new tail",
                displayTranscript = "old session. new tail",
                lastCommittedPos = 0,
            ),
        )

        assertEquals("old session.", transcriptOldText(state))
        assertEquals(" new tail", transcriptNewText(state))
    }

    @Test
    fun `transcript split joins frozen prefix with committed fresh text`() {
        val state = LiveSessionState(
            liveText = LiveTextState(
                frozenPrefix = "old session.",
                fullTranscript = "new tail",
                displayTranscript = "old session. new tail",
                lastCommittedPos = 3,
            ),
        )

        assertEquals("old session. new", transcriptOldText(state))
        assertEquals(" tail", transcriptNewText(state))
    }
}
