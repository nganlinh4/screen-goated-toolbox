package dev.screengoated.toolbox.mobile.phonecontrol.provider.visual

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class VisualScreenshotCachePolicyTest {
    @Test
    fun reuseTracksThePlatformCaptureInterval() {
        assertTrue(shouldReuseVisualScreenshot(7, 10_000, 7, 11_000, apiLevel = 30))
        assertFalse(shouldReuseVisualScreenshot(7, 10_000, 7, 11_001, apiLevel = 30))
        assertTrue(shouldReuseVisualScreenshot(7, 10_000, 7, 10_333, apiLevel = 31))
        assertFalse(shouldReuseVisualScreenshot(7, 10_000, 7, 10_334, apiLevel = 31))
    }

    @Test
    fun reuseNeverCrossesGenerationOrClockDiscontinuity() {
        assertFalse(shouldReuseVisualScreenshot(7, 10_000, 8, 10_001, apiLevel = 36))
        assertFalse(shouldReuseVisualScreenshot(7, 10_000, 7, 9_999, apiLevel = 36))
    }
}
