package dev.screengoated.toolbox.mobile.phonecontrol.projection

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PhoneControlProjectionLifecycleTest {
    @Test
    fun `decode diagnostics report transitions and bounded summaries only`() {
        assertTrue(shouldSummarizeProjectionDecodeFailure(1))
        assertFalse(shouldSummarizeProjectionDecodeFailure(2))
        assertFalse(shouldSummarizeProjectionDecodeFailure(299))
        assertTrue(shouldSummarizeProjectionDecodeFailure(300))
        assertTrue(shouldSummarizeProjectionDecodeFailure(600))
    }
}
