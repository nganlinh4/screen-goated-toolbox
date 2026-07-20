package dev.screengoated.toolbox.mobile.phonecontrol.overlay

import dev.screengoated.toolbox.mobile.phonecontrol.GeneratedPhoneControlContract
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlServiceState
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlRuntimeCode
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlRuntimePhase
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PhoneControlOverlayStateTest {
    @Test
    fun `stopped and failed sessions cannot leave an orb behind`() {
        assertFalse(visual(PhoneControlRuntimePhase.STOPPED, running = false).visible)
        assertFalse(visual(PhoneControlRuntimePhase.ERROR, running = false).visible)
    }

    @Test
    fun `listening clears captions from the retired response generation`() {
        val result = visual(
            phase = PhoneControlRuntimePhase.LISTENING,
            input = "old request",
            output = "old response",
        )

        assertEquals(GeneratedPhoneControlContract.ORB_STATE_IDLE, result.stateLabel)
        assertEquals("", result.caption)
    }

    @Test
    fun `working and finalizing expose only the current useful caption`() {
        assertEquals(
            "request",
            visual(PhoneControlRuntimePhase.WORKING, input = "request").caption,
        )
        assertEquals(
            "partial response",
            visual(
                PhoneControlRuntimePhase.WORKING,
                input = "request",
                output = "partial response",
            ).caption,
        )
        assertEquals(
            "final response",
            visual(
                PhoneControlRuntimePhase.FINALIZING,
                input = "request",
                output = "final response",
            ).caption,
        )
    }

    @Test
    fun `overlay preserves a stable streaming caption prefix`() {
        val response = "word ".repeat(150).trim()

        assertEquals(
            response,
            visual(PhoneControlRuntimePhase.WORKING, output = response).caption,
        )
    }

    @Test
    fun `working state preserves generated renderer state and icon override`() {
        val result = visual(
            phase = PhoneControlRuntimePhase.WORKING,
            orbState = GeneratedPhoneControlContract.ORB_STATE_SCROLL,
            orbIcon = "keyboard_double_arrow_up",
        )

        assertEquals(GeneratedPhoneControlContract.ORB_STATE_SCROLL, result.stateLabel)
        assertEquals("keyboard_double_arrow_up", result.iconOverride)
    }

    @Test
    fun `connecting and degraded states keep actionable status with bounded level`() {
        val connecting = visual(PhoneControlRuntimePhase.CONNECTING, message = "Connecting")
        val degraded = visual(
            PhoneControlRuntimePhase.DEGRADED,
            message = "Reconnect Accessibility",
            level = 4f,
        )

        assertEquals("Connecting", connecting.caption)
        assertEquals(GeneratedPhoneControlContract.ORB_STATE_ERROR, degraded.stateLabel)
        assertEquals("Reconnect Accessibility", degraded.caption)
        assertEquals(1f, degraded.listeningLevel)
        assertTrue(degraded.visible)
    }

    private fun visual(
        phase: PhoneControlRuntimePhase,
        running: Boolean = true,
        message: String = "status",
        input: String = "",
        output: String = "",
        level: Float = 0f,
        orbState: String = GeneratedPhoneControlContract.ORB_STATE_THINKING,
        orbIcon: String? = null,
    ) = phoneControlOverlayVisual(
        PhoneControlServiceState(
            running = running,
            phase = phase,
            code = PhoneControlRuntimeCode.READY,
            userMessage = message,
            inputCaption = input,
            outputCaption = output,
            listeningLevel = level,
            orbStateLabel = orbState,
            orbIconOverride = orbIcon,
        ),
    )
}
