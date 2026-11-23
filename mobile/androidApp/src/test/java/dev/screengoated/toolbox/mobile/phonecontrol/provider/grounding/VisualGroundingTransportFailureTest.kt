package dev.screengoated.toolbox.mobile.phonecontrol.provider.grounding

import java.io.IOException
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class VisualGroundingTransportFailureTest {
    @Test
    fun authFailure_surfacesConfigureKeyStep() {
        // VisionApiClientGemini maps HTTP 401/403 to this message.
        val failure = groundingTransportFailure(IOException("INVALID_API_KEY:google"))
        assertEquals("capability_unavailable", failure.code)
        assertFalse(failure.retryable)
        assertEquals("configure_gemini_api_key", failure.requiredUserStep)

        val raw = groundingTransportFailure(IOException("Gemini vision failed with 403 PERMISSION_DENIED"))
        assertFalse(raw.retryable)
        assertEquals("configure_gemini_api_key", raw.requiredUserStep)
    }

    @Test
    fun rateLimit_isTypedAndRetryable() {
        val failure = groundingTransportFailure(IOException("Gemini vision failed with 429 Resource exhausted"))
        assertEquals("rate_limited", failure.code)
        assertTrue(failure.retryable)
        assertNull(failure.requiredUserStep)
    }

    @Test
    fun otherTransportFailure_keepsDetail() {
        val failure = groundingTransportFailure(IOException("connection reset"))
        assertEquals("vision_grounding_failed", failure.code)
        assertTrue(failure.retryable)
        assertTrue(failure.message.contains("connection reset"))
    }
}
