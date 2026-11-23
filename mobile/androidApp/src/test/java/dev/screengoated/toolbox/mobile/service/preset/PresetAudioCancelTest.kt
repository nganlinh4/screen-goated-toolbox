package dev.screengoated.toolbox.mobile.service.preset

import org.junit.Assert.assertEquals
import org.junit.Test

class PresetAudioCancelTest {
    @Test fun cancellationNotifiesOriginalOwnerAfterTeardownClearsCallback() {
        val events = mutableListOf<String>()
        var callback: (() -> Unit)? = { events += "foreground released" }
        cancelAudioCaptureAndNotify(callback) {
            events += "capture stopped"
            callback = null
        }
        assertEquals(listOf("capture stopped", "foreground released"), events)
    }
}
