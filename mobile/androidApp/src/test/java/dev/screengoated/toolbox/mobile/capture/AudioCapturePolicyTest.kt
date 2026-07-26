package dev.screengoated.toolbox.mobile.capture

import org.junit.Assert.assertEquals
import org.junit.Test

class AudioCapturePolicyTest {
    @Test
    fun `platform read failures have bounded structural diagnostic codes`() {
        assertEquals("dead_object", AudioCaptureReadException(-6).diagnosticCode)
        assertEquals("invalid_operation", AudioCaptureReadException(-3).diagnosticCode)
        assertEquals("bad_value", AudioCaptureReadException(-2).diagnosticCode)
        assertEquals("error", AudioCaptureReadException(-1).diagnosticCode)
        assertEquals("unknown", AudioCaptureReadException(-999).diagnosticCode)
    }
}
