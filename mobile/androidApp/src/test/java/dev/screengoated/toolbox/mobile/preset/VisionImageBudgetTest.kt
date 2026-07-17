package dev.screengoated.toolbox.mobile.preset

import java.io.IOException
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.Paths
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Assert.assertFalse
import org.junit.Test

class VisionImageBudgetTest {
    @Test
    fun qwenKeepsDefaultReasoningButHidesReasoningOutput() {
        val payload = openAiVisionPayload(
            fullName = "qwen/qwen3.6-27b",
            prompt = "Read this image",
            imageBase64 = "AA==",
            mimeType = "image/png",
            stream = false,
        )

        assertEquals("hidden", payload.getString("reasoning_format"))
        assertFalse(payload.has("reasoning_effort"))
        assertEquals(2048, payload.getInt("max_completion_tokens"))
    }

    @Test
    fun limitsMatchWindowsParityFixture() {
        val fixture = Files.readAllBytes(fixturePath()).decodeToString()
        val groq = JSONObject(fixture).getJSONObject("groq")

        assertEquals(3_800_000, groq.getInt("safe_request_bytes"))
        assertEquals(16_384, groq.getInt("json_reserve_bytes"))
        assertEquals(2_500_000, groq.getInt("maximum_encoded_image_bytes"))
        assertEquals(262_144, groq.getInt("minimum_encoded_image_bytes"))
        val qwen = groq.getJSONObject("qwen_portable_tpm")
        assertEquals(8_000, qwen.getInt("limit"))
        assertEquals(2_048, qwen.getInt("completion_token_reserve"))
        assertEquals(3_072, qwen.getInt("image_and_envelope_token_reserve"))
        assertEquals(3, qwen.getInt("estimated_prompt_bytes_per_token"))
    }

    @Test
    fun budgetLeavesRoomForBase64PromptAndJson() {
        val promptBytes = 32_000
        val imageBytes = groqImageByteBudget(promptBytes)
        val base64Bytes = ((imageBytes + 2) / 3) * 4

        assertTrue(base64Bytes + promptBytes + 16_384 <= 3_800_000)
        assertEquals(2_500_000, imageBytes)
    }

    @Test
    fun oversizedPromptFailsBeforeNetworkRequest() {
        assertThrows(IOException::class.java) {
            groqImageByteBudget(3_800_000)
        }
    }

    @Test
    fun qwenTpmOversizeFailsBeforeImageEncodingOrNetworkRequest() {
        assertThrows(IOException::class.java) {
            ensureQwenPromptFitsPortableTpm(60_000)
        }
        ensureQwenPromptFitsPortableTpm(1_000)
    }

    @Test
    fun groqRetriesOneShortRateLimitWaitOnly() {
        assertEquals(15_000L, groqVisionRetryDelayMillis("Groq", 429, false, 15))
        assertEquals(null, groqVisionRetryDelayMillis("Groq", 429, true, 15))
        assertEquals(null, groqVisionRetryDelayMillis("Groq", 429, false, 31))
        assertEquals(null, groqVisionRetryDelayMillis("Cerebras", 429, false, 15))
    }

    private fun fixturePath(): Path {
        return listOf(
            Paths.get("..", "parity-fixtures", "preset-system", "vision-payload.json"),
            Paths.get("..", "..", "parity-fixtures", "preset-system", "vision-payload.json"),
            Paths.get("parity-fixtures", "preset-system", "vision-payload.json"),
        ).firstOrNull(Files::exists)
            ?: error("Unable to locate preset-system/vision-payload.json")
    }
}
