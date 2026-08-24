package dev.screengoated.toolbox.mobile.preset

import okhttp3.Protocol
import okhttp3.Request
import okhttp3.Response
import okhttp3.ResponseBody.Companion.toResponseBody
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

class PresetHttpPolicyTest {
    @Test
    fun unaryTimeoutUsesStructuralWorkloadInsteadOfBenchmarkLatency() {
        assertEquals(60_000L, workloadDerivedTimeoutMillis(0, 1))
        assertEquals(102_000L, workloadDerivedTimeoutMillis(1_000_000, 160))
        assertEquals(900_000L, workloadDerivedTimeoutMillis(Long.MAX_VALUE, Long.MAX_VALUE))
    }

    @Test
    fun streamingHasProgressIdleDeadlineWithoutWholeCallDeadline() {
        val model = requireNotNull(PresetModelCatalog.getById("google-gemini-3-5-flash-lite-text"))
        val policy = presetRequestDeadlinePolicy(model, streamingEnabled = true, encodedRequestBytes = 0)

        assertEquals(120_000L, policy.readIdleTimeoutMillis)
        assertNull(policy.wholeCallTimeoutMillis)
    }

    @Test
    fun bufferedTextGetsOutputAndRequestAllowances() {
        val model = requireNotNull(PresetModelCatalog.getById("google-gemini-3-5-flash-lite-text"))
        val policy = presetRequestDeadlinePolicy(model, streamingEnabled = false, encodedRequestBytes = 1_000_000)

        assertEquals(348_000L, policy.wholeCallTimeoutMillis)
        assertEquals(policy.wholeCallTimeoutMillis, policy.readIdleTimeoutMillis)
    }

    @Test
    fun deadlinePolicyMatchesSharedParityFixture() {
        val root = generateSequence(File(requireNotNull(System.getProperty("user.dir"))).absoluteFile) {
            it.parentFile
        }.first { File(it, "parity-fixtures/preset-system/retry-runtime.json").exists() }
        val fixture = JSONObject(
            File(root, "parity-fixtures/preset-system/retry-runtime.json").readText(),
        ).getJSONObject("interactive_deadlines")
        val streaming = fixture.getJSONObject("streaming")
        val unary = fixture.getJSONObject("non_streaming")

        assertEquals(120_000L, streaming.getLong("response_start_timeout_ms"))
        assertEquals(120_000L, streaming.getLong("progress_idle_timeout_ms"))
        assertTrue(streaming.isNull("whole_call_timeout_ms"))
        assertEquals(30_000L, unary.getLong("startup_allowance_ms"))
        assertEquals(16_384L, unary.getLong("request_bytes_per_allowance_second"))
        assertEquals(120_000L, unary.getLong("maximum_request_allowance_ms"))
        assertEquals(16L, unary.getLong("minimum_output_tokens_per_second"))
        assertEquals(4_096L, unary.getLong("default_text_output_tokens"))
        assertEquals(2_048L, unary.getLong("default_vision_output_tokens"))
        assertEquals(60_000L, unary.getLong("minimum_hard_timeout_ms"))
        assertEquals(900_000L, unary.getLong("maximum_hard_timeout_ms"))
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
