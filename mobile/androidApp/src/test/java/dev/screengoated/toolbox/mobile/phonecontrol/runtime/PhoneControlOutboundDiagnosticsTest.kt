package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlTurnPhase
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PhoneControlOutboundDiagnosticsTest {
    @Test
    fun `pending function call pauses ambient screen but not microphone audio`() {
        assertTrue(canSendAmbientScreen(0))
        assertFalse(canSendAmbientScreen(1))
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
