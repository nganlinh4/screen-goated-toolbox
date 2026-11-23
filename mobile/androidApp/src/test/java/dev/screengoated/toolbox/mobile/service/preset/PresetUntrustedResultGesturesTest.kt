package dev.screengoated.toolbox.mobile.service.preset

import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import dev.screengoated.toolbox.mobile.preset.PresetResultWindowId
import dev.screengoated.toolbox.mobile.preset.PresetResultWindowState

class PresetUntrustedResultGesturesTest {
    @Test
    fun audioDownloadUsesOnlyTheCurrentUnchangedInputResult() {
        val bytes = byteArrayOf(1, 2, 3)
        val state = PresetResultWindowState(
            id = PresetResultWindowId("session", 1), blockIdx = 1, title = "result",
            markdownText = "original", inputAudioBytes = bytes, inputAudioHtml = java.lang.ref.WeakReference("original"),
        )
        assertSame(bytes, inputAudioForDownload(state))
        assertNull(inputAudioForDownload(state.copy(markdownText = "replaced")))
        assertNull(inputAudioForDownload(state.copy(inputAudioBytes = null)))
        assertNull(inputAudioForDownload(state.copy(inputAudioHtml = null)))
    }
    @Test
    fun onlyCurrentMainFrameMaySendAllowedGestureMessages() {
        val valid = """{"type":"dragResultWindow","windowId":"session:1","dx":3,"dy":4}"""
        assertTrue(allowUntrustedResultGestureMessage(valid, "session:1", true))
        assertFalse(allowUntrustedResultGestureMessage(valid, "session:1", false))
        assertFalse(allowUntrustedResultGestureMessage(valid, "session:2", true))
        assertFalse(allowUntrustedResultGestureMessage("not json", "session:1", true))
        assertFalse(allowUntrustedResultGestureMessage(
            """{"type":"saveMediaToDownloads","windowId":"session:1"}""", "session:1", true,
        ))
        assertTrue(allowUntrustedResultGestureMessage(
            """{"type":"downloadInputAudio","windowId":"session:1"}""", "session:1", true,
        ))
        assertFalse(allowUntrustedResultGestureMessage(
            """{"type":"navigationState","windowId":"session:1"}""", "session:1", true,
        ))
    }

    @Test
    fun consequentialGestureHandlersRejectSyntheticEvents() {
        val touch = presetResultJavascriptTouchSupport()
        val selection = presetResultSelectionOverlayJavascript()
        for (eventName in listOf("click", "touchstart", "touchmove", "touchend", "touchcancel")) {
            val handler = touch.substringAfter("document.addEventListener('$eventName', event => {")
            assertTrue("$eventName must check event trust", handler.trimStart().startsWith("if (!event.isTrusted) return;"))
        }
        assertTrue(selection.substringAfter("action.addEventListener('click', event => {")
            .trimStart().startsWith("if (!event.isTrusted) return;"))
    }
}
