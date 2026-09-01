package dev.screengoated.toolbox.mobile.preset

import okhttp3.Protocol
import okhttp3.Request
import okhttp3.Response
import okhttp3.ResponseBody.Companion.toResponseBody
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

class PresetHttpPolicyTest {
    @Test
    fun benchmarkLatencyAndRequestBytesBoundEveryInteractiveAttempt() {
        val base = requireNotNull(PresetModelCatalog.getById("groq-qwen-3-8-27b-text"))
        val qwen38 = base.copy(typicalLatencyMs = 1_195)
        val qwen36 = base.copy(typicalLatencyMs = 1_788)

        val fast = presetRequestDeadlinePolicy(qwen38, encodedRequestBytes = 4_487)
        assertEquals(3_603L, fast.responseStartTimeoutMillis)
        assertEquals(2_390L, fast.progressIdleTimeoutMillis)
        assertEquals(5_000L, fast.attemptTimeoutMillis)
        assertEquals(8_000L, presetChainTimeoutMillis(qwen38, 4_487))

        val fallback = presetRequestDeadlinePolicy(qwen36, encodedRequestBytes = 4_487)
        assertEquals(5_382L, fallback.responseStartTimeoutMillis)
        assertEquals(3_576L, fallback.progressIdleTimeoutMillis)
        assertEquals(7_170L, fallback.attemptTimeoutMillis)

        val boundedModel = base.copy(typicalLatencyMs = Int.MAX_VALUE)
        val bounded = presetRequestDeadlinePolicy(boundedModel, encodedRequestBytes = Long.MAX_VALUE)
        assertEquals(15_000L, bounded.responseStartTimeoutMillis)
        assertEquals(8_000L, bounded.progressIdleTimeoutMillis)
        assertEquals(30_000L, bounded.attemptTimeoutMillis)
        assertEquals(30_000L, presetChainTimeoutMillis(boundedModel, Long.MAX_VALUE))
    }

    @Test
    fun remainingChainBudgetCapsEveryHttpPhase() {
        val model = requireNotNull(PresetModelCatalog.getById("groq-qwen-3-8-27b-text"))
            .copy(typicalLatencyMs = 1_195)
        val capped = presetRequestDeadlinePolicy(model, 4_487).cappedBy(900L)

        assertEquals(900L, capped.connectTimeoutMillis)
        assertEquals(900L, capped.sendTimeoutMillis)
        assertEquals(900L, capped.responseStartTimeoutMillis)
        assertEquals(900L, capped.progressIdleTimeoutMillis)
        assertEquals(900L, capped.attemptTimeoutMillis)
    }

    @Test
    fun deadlinePolicyMatchesSharedParityFixture() {
        val root = generateSequence(File(requireNotNull(System.getProperty("user.dir"))).absoluteFile) {
            it.parentFile
        }.first { File(it, "parity-fixtures/preset-system/retry-runtime.json").exists() }
        val fixture = JSONObject(
            File(root, "parity-fixtures/preset-system/retry-runtime.json").readText(),
        ).getJSONObject("interactive_deadlines")
        assertEquals(3_000L, fixture.getLong("default_latency_ms"))
        assertEquals(262_144L, fixture.getLong("request_bytes_per_allowance_second"))
        assertEquals(3_000L, fixture.getLong("maximum_request_allowance_ms"))
        assertEquals(5_000L, fixture.getLong("connect_timeout_ms"))
        assertEquals(2_000L, fixture.getLong("send_base_timeout_ms"))
        assertEquals(10_000L, fixture.getLong("maximum_send_timeout_ms"))
        assertEquals(3L, fixture.getLong("response_start_latency_multiplier"))
        assertEquals(2L, fixture.getLong("progress_idle_latency_multiplier"))
        assertEquals(4L, fixture.getLong("attempt_latency_multiplier"))
        assertEquals(5L, fixture.getLong("chain_latency_multiplier"))
        assertEquals(5_000L, fixture.getLong("minimum_attempt_timeout_ms"))
        assertEquals(30_000L, fixture.getLong("maximum_attempt_timeout_ms"))
        assertEquals(8_000L, fixture.getLong("minimum_chain_timeout_ms"))
        assertEquals(30_000L, fixture.getLong("maximum_chain_timeout_ms"))
        assertTrue(fixture.getBoolean("presentation_transport_streaming_separate"))
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
