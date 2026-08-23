package dev.screengoated.toolbox.mobile.preset

import java.io.IOException
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.Paths
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

class VisionImageBudgetTest {
    @Test
    fun endpointProfilesOwnVisionPartOrderAndDefaultResolution() {
        val googleGemma =
            requireNotNull(PresetModelCatalog.getById("google-gemma-4-31b-vision"))
        val gemmaPayload = buildGeminiVisionPayload(
            model = googleGemma,
            prompt = "Read",
            imageBase64 = "AA==",
            mimeType = "image/png",
        )
        val gemmaParts = gemmaPayload
            .getJSONArray("contents")
            .getJSONObject(0)
            .getJSONArray("parts")
        assertTrue(gemmaParts.getJSONObject(0).has("inline_data"))
        assertEquals(
            PresetVisionMediaResolution.PROVIDER_DEFAULT,
            googleGemma.visionMediaResolution,
        )
        assertFalse(
            gemmaPayload
                .getJSONObject("generationConfig")
                .has("mediaResolution"),
        )

        val flash =
            requireNotNull(PresetModelCatalog.getById("google-gemini-3-5-flash-lite-vision"))
        val flashParts = buildGeminiVisionPayload(
            model = flash,
            prompt = "Read",
            imageBase64 = "AA==",
            mimeType = "image/png",
        ).getJSONArray("contents")
            .getJSONObject(0)
            .getJSONArray("parts")
        assertTrue(flashParts.getJSONObject(0).has("inline_data"))
        assertEquals("Read", flashParts.getJSONObject(1).getString("text"))

        val schema = JSONObject("""{"type":"object"}""")
        val structured = buildGeminiVisionPayload(
            model = flash,
            prompt = "Locate",
            imageBase64 = "AA==",
            mimeType = "image/png",
            responseSchema = schema,
        ).getJSONObject("generationConfig")
        assertEquals("application/json", structured.getString("responseMimeType"))
        assertEquals(schema.toString(), structured.getJSONObject("responseJsonSchema").toString())
    }

    @Test
    fun qwenDisablesReasoningAndHidesAnyReasoningOutput() {
        val payload = openAiVisionPayload(
            model = requireNotNull(PresetModelCatalog.getById("groq-qwen-3-6-27b-vision")),
            prompt = "Read this image",
            imageBase64 = "AA==",
            mimeType = "image/png",
            stream = false,
        )

        assertEquals("hidden", payload.getString("reasoning_format"))
        assertEquals("none", payload.getString("reasoning_effort"))
        assertEquals(512, payload.getInt("max_completion_tokens"))
        assertEquals(0.7, payload.getDouble("temperature"), 0.0)
        assertEquals(0.8, payload.getDouble("top_p"), 0.0)
        assertEquals(1.5, payload.getDouble("presence_penalty"), 0.0)
        assertFalse(payload.has("top_k"))
        assertFalse(payload.has("min_p"))
        assertFalse(payload.has("response_format"))
        assertTrue(
            requireNotNull(PresetModelCatalog.getById("groq-qwen-3-6-27b-vision"))
                .restatesOutput,
        )
    }

    @Test
    fun nvidiaVisionUsesDeterministicSampling() {
        val model = PresetModelDescriptor(
            id = "nvidia-test-vision",
            provider = PresetModelProvider.NVIDIA,
            fullName = "nvidia/test-vision",
            modelType = PresetModelType.VISION,
            displayName = "N test",
        )
        val payload = openAiVisionPayload(
            model = model,
            prompt = "Read",
            imageBase64 = "AA==",
            mimeType = "image/png",
            stream = false,
        )
        assertEquals(0.0, payload.getDouble("temperature"), 0.0)
    }

    @Test
    fun openRouterGemmaUsesNestedReasoningAndTextFirstInput() {
        val payload = openAiVisionPayload(
            model = requireNotNull(
                PresetModelCatalog.getById(
                    "openrouter-gemma-4-26b-a4b-vision",
                ),
            ),
            prompt = "Read this image",
            imageBase64 = "AA==",
            mimeType = "image/png",
            stream = false,
        )

        assertEquals("none", payload.getJSONObject("reasoning").getString("effort"))
        assertFalse(payload.has("reasoning_effort"))
        assertEquals(
            "text",
            payload.getJSONArray("messages")
                .getJSONObject(0)
                .getJSONArray("content")
                .getJSONObject(0)
                .getString("type"),
        )
    }

    @Test
    fun limitsMatchWindowsParityFixture() {
        val fixture = Files.readAllBytes(fixturePath()).decodeToString()
        val root = JSONObject(fixture)
        val groq = root.getJSONObject("groq")

        assertEquals(3_800_000, groq.getInt("safe_request_bytes"))
        assertEquals(16_384, groq.getInt("json_reserve_bytes"))
        assertEquals(2_500_000, groq.getInt("maximum_encoded_image_bytes"))
        assertEquals(262_144, groq.getInt("minimum_encoded_image_bytes"))
        val qwen = groq.getJSONObject("qwen_portable_tpm")
        assertEquals(8_000, qwen.getInt("limit"))
        assertEquals(512, qwen.getInt("completion_token_reserve"))
        assertEquals(3_072, qwen.getInt("image_and_envelope_token_reserve"))
        assertEquals(3, qwen.getInt("estimated_prompt_bytes_per_token"))
        assertEquals(2, groq.getInt("short_retry_after_max_seconds"))

        val ordinary = root.getJSONObject("ordinary_llm_profiles")
        assertEquals("non-streaming", ordinary.getString("ocr_transport"))
        val cases = ordinary.getJSONArray("cases")
        for (index in 0 until cases.length()) {
            val case = cases.getJSONObject(index)
            val provider = PresetModelProvider.valueOf(
                case.getString("provider").uppercase().replace('-', '_'),
            )
            val model = PresetModelCatalog.runtimeModels().first {
                it.provider == provider && it.fullName == case.getString("api_model")
            }
            assertEquals(case.getString("input_order"), model.visionInputOrder.wireName())
            assertEquals(
                case.getString("media_resolution"),
                model.visionMediaResolution.wireName(),
            )
            assertEquals(case.getString("sampling"), model.visionSamplingPolicy.wireName())
            if (case.isNull("max_output_tokens")) {
                assertEquals(null, model.visionMaxOutputTokens)
            } else {
                assertEquals(case.getInt("max_output_tokens"), model.visionMaxOutputTokens)
            }
            assertEquals(
                case.getString("structured_output"),
                model.structuredOutputPolicy.wireName(),
            )
            assertEquals(case.getBoolean("restates_output"), model.restatesOutput)
        }
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
            ensureQwenPromptFitsPortableTpm(60_000, 512)
        }
        ensureQwenPromptFitsPortableTpm(1_000, 512)
    }

    @Test
    fun groqRetriesOneShortRateLimitWaitOnly() {
        assertEquals(2_000L, groqVisionRetryDelayMillis("Groq", 429, false, 2))
        assertEquals(null, groqVisionRetryDelayMillis("Groq", 429, true, 2))
        assertEquals(null, groqVisionRetryDelayMillis("Groq", 429, false, 3))
        assertEquals(null, groqVisionRetryDelayMillis("OpenRouter", 429, false, 2))
    }

    private fun fixturePath(): Path {
        return listOf(
            Paths.get("..", "parity-fixtures", "preset-system", "vision-payload.json"),
            Paths.get("..", "..", "parity-fixtures", "preset-system", "vision-payload.json"),
            Paths.get("parity-fixtures", "preset-system", "vision-payload.json"),
        ).firstOrNull(Files::exists)
            ?: error("Unable to locate preset-system/vision-payload.json")
    }

    private fun Enum<*>.wireName(): String = name.lowercase().replace('_', '-')
}
