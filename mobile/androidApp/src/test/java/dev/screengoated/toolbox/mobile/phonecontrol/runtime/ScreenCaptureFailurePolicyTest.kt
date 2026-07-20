package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ScreenCaptureFailurePolicyTest {
    @Test
    fun `frame races and absence of an external surface stay transient`() {
        listOf(
            "surface_unavailable",
            "surface_unstable",
            "stale_frame",
            "screenshot_rate_limited",
        ).forEach { code -> assertTrue(code, isTransientScreenFrameFailure(code)) }
    }

    @Test
    fun `platform and processing failures remain visible`() {
        listOf(
            "screenshot_capability_missing",
            "screenshot_secure_window",
            "screenshot_request_failed",
            "screenshot_processing_failed",
            "unsupported_display",
        ).forEach { code -> assertFalse(code, isTransientScreenFrameFailure(code)) }
    }
}
