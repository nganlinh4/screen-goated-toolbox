package dev.screengoated.toolbox.mobile.preset

import okhttp3.Protocol
import okhttp3.Request
import okhttp3.Response
import okhttp3.ResponseBody.Companion.toResponseBody
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class PresetHttpPolicyTest {
    @Test
    fun nonStreamingTimeoutUsesBenchmarkLatencyWithinSafetyBounds() {
        assertEquals(10_000L, benchmarkDerivedTimeoutMillis(300, streamingEnabled = false))
        assertEquals(18_000L, benchmarkDerivedTimeoutMillis(1_800, streamingEnabled = false))
        assertEquals(30_000L, benchmarkDerivedTimeoutMillis(9_000, streamingEnabled = false))
        assertEquals(30_000L, benchmarkDerivedTimeoutMillis(null, streamingEnabled = false))
        assertNull(benchmarkDerivedTimeoutMillis(1_800, streamingEnabled = true))
    }

    @Test
    fun providerFailurePreservesStructuredBodyAndRetryHintForCircuitClassification() {
        val response = Response.Builder()
            .request(Request.Builder().url("https://example.test/v1").build())
            .protocol(Protocol.HTTP_1_1)
            .code(429)
            .message("Too Many Requests")
            .header("retry-after", "22.012")
            .body("""{"error":{"message":"quota exceeded"}}""".toResponseBody())
            .build()

        val message = response.use { it.providerFailureMessage("Provider request") }
        assertTrue(message.contains("429"))
        assertTrue(message.contains("quota exceeded"))
        assertTrue(message.contains("retry-after: 22.012"))
        assertEquals(22_012L, reportedCooldownMillis(message))
    }
}
