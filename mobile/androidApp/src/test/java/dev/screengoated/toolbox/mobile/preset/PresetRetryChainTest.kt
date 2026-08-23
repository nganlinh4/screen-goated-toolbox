package dev.screengoated.toolbox.mobile.preset

import dev.screengoated.toolbox.mobile.shared.preset.DEFAULT_IMAGE_MODEL_ID
import dev.screengoated.toolbox.mobile.shared.preset.DEFAULT_TEXT_MODEL_ID
import dev.screengoated.toolbox.mobile.shared.preset.DefaultPresets
import dev.screengoated.toolbox.mobile.shared.preset.PRESET_AUDIO_CONTINUOUS_MODEL_ID
import dev.screengoated.toolbox.mobile.shared.preset.PRESET_AUDIO_DIRECT_TRANSLATE_MODEL_ID
import dev.screengoated.toolbox.mobile.shared.preset.PRESET_AUDIO_OFFLINE_TRANSCRIBE_MODEL_ID
import dev.screengoated.toolbox.mobile.shared.preset.PRESET_AUDIO_TRANSCRIBE_MODEL_ID
import dev.screengoated.toolbox.mobile.shared.preset.PRESET_SEARCH_MODEL_ID
import dev.screengoated.toolbox.mobile.shared.preset.PRESET_TEXT_ARENA_FAST_MODEL_ID
import dev.screengoated.toolbox.mobile.shared.preset.PRESET_TEXT_GAME_MODEL_ID
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.Paths

class PresetRetryChainTest {
    private val json = Json { ignoreUnknownKeys = true }

    @After
    fun clearCircuitState() {
        clearPresetModelCircuitsForTest()
    }

    @Test
    fun advancesRetryChainForRetryableErrorsLikeWindows() {
        assertTrue(shouldAdvanceRetryChain("NO_API_KEY:google"))
        assertTrue(shouldAdvanceRetryChain("INVALID_API_KEY"))
        assertTrue(shouldAdvanceRetryChain("Gemini request failed with 400"))
        assertTrue(shouldAdvanceRetryChain("request failed with status code 404"))
        assertTrue(shouldAdvanceRetryChain("unsupported model"))
        assertFalse(shouldAdvanceRetryChain("request failed with 200"))
    }

    @Test
    fun blocksProviderOnlyForAuthAndProviderAvailabilityErrors() {
        assertTrue(shouldBlockRetryProvider("NO_API_KEY:groq"))
        assertTrue(shouldBlockRetryProvider("INVALID_API_KEY"))
        assertTrue(shouldBlockRetryProvider("PROVIDER_DISABLED:google"))
        assertTrue(shouldBlockRetryProvider("PROVIDER_NOT_READY:gemini-live"))
        assertTrue(shouldBlockRetryProvider("request failed with status code 403"))
        assertFalse(shouldBlockRetryProvider("request failed with status code 404"))
    }

    @Test
    fun preflightSkipsMissingProviderCredentials() {
        assertEquals(
            "NO_API_KEY:google",
            preflightSkipReason(
                modelId = "google-gemini-3-flash-text",
                provider = PresetModelProvider.GOOGLE,
                apiKeys = ApiKeys(),
                blockedProviders = emptySet(),
                settings = PresetRuntimeSettings(),
            ),
        )
    }

    @Test
    fun preflightSkipsModelDuringRateLimitCooldown() {
        val modelId = "rate-limited-test-model"
        recordPresetModelFailure(modelId, "vision request failed with 429: quota exceeded")

        val reason = preflightSkipReason(
            modelId = modelId,
            provider = PresetModelProvider.GROQ,
            apiKeys = ApiKeys(groqKey = "g"),
            blockedProviders = emptySet(),
            settings = PresetRuntimeSettings(),
        )

        assertTrue(reason?.startsWith("MODEL_RATE_LIMIT_COOLDOWN:$modelId:") == true)
    }

    @Test
    fun reportedCooldownParsingMatchesProviderFormatsAndSafetyBounds() {
        assertEquals(22_012L, reportedCooldownMillis("HTTP 429; please try again in 22.012s"))
        assertEquals(90_000L, reportedCooldownMillis("quota exceeded; retry in 1m30s"))
        assertEquals(22_000L, reportedCooldownMillis("HTTP 429 retry-after: 22"))
        assertEquals(5_000L, reportedCooldownMillis("429 try again in 500ms"))
        assertEquals(21_600_000L, reportedCooldownMillis("429 retry after 99h"))
        assertNull(reportedCooldownMillis("429 rate limit reached"))
    }

    @Test
    fun twoTimeoutsOpenCircuitAndSuccessClearsIt() {
        val modelId = "timeout-circuit-model"
        recordPresetModelFailureAt(modelId, "request timed out", 1_000L)
        assertNull(presetModelCircuitSkipReasonAt(modelId, 1_001L))

        recordPresetModelFailureAt(modelId, "deadline exceeded", 2_000L)
        assertTrue(
            presetModelCircuitSkipReasonAt(modelId, 2_001L)
                ?.startsWith("MODEL_TIMEOUT_COOLDOWN:$modelId:") == true,
        )

        recordPresetModelSuccess(modelId)
        assertNull(presetModelCircuitSkipReasonAt(modelId, 2_002L))
    }

    @Test
    fun expiredCircuitAdmitsExactlyOneHalfOpenProbe() {
        val modelId = "half-open-model"
        recordPresetModelFailureAt(modelId, "HTTP 429 retry-after: 5", 10_000L)

        assertNull(claimPresetModelAttemptAt(modelId, 15_000L))
        assertEquals(
            "MODEL_COOLDOWN_PROBE_IN_FLIGHT:$modelId",
            claimPresetModelAttemptAt(modelId, 15_000L),
        )

        releasePresetModelProbeAt(modelId, 15_000L)
        assertNull(claimPresetModelAttemptAt(modelId, 15_000L))
    }

    @Test
    fun unavailableAndBillingFailuresUseLongLivedTypedCircuits() {
        val unavailable = "withdrawn-model"
        val billing = "paid-model"
        recordPresetModelFailureAt(
            unavailable,
            "HTTP 404 model not found because it was removed",
            4_000L,
        )
        recordPresetModelFailureAt(billing, "HTTP 402 payment required", 4_000L)

        assertTrue(
            presetModelCircuitSkipReasonAt(unavailable, 4_001L)
                ?.startsWith("MODEL_UNAVAILABLE_COOLDOWN:$unavailable:") == true,
        )
        assertTrue(
            presetModelCircuitSkipReasonAt(billing, 4_001L)
                ?.startsWith("MODEL_BILLING_COOLDOWN:$billing:") == true,
        )
    }

    @Test
    fun retryResolutionUsesWindowsDefaultChainFirst() {
        val next = resolveNextRetryModel(
            currentModelId = "google-gemma-4-26b-a4b-text",
            failedModelIds = listOf("google-gemma-4-26b-a4b-text"),
            blockedProviders = emptySet(),
            chainKind = PresetRetryChainKind.TEXT_TO_TEXT,
            apiKeys = ApiKeys(
                geminiKey = "g",
                groqKey = "r",
                openRouterKey = "o",
                ollamaBaseUrl = "http://localhost:11434",
            ),
            settings = PresetRuntimeSettings(),
        )

        assertNotNull(next)
        assertEquals(
            GeneratedPresetModelCatalogData.modelPriorityChains.textToText.first(),
            next?.id,
        )
    }

    @Test
    fun disabledProviderIsSkippedLikeWindowsConfig() {
        assertEquals(
            "PROVIDER_DISABLED:google",
            preflightSkipReason(
                modelId = "google-gemini-3-flash-text",
                provider = PresetModelProvider.GOOGLE,
                apiKeys = ApiKeys(geminiKey = "g"),
                blockedProviders = emptySet(),
                settings = PresetRuntimeSettings(
                    providerSettings = PresetProviderSettings(useGemini = false),
                ),
            ),
        )
    }

    @Test
    fun generatedDefaultsMatchWindowsRetryFixture() {
        val root = json.parseToJsonElement(Files.readAllBytes(fixturePath()).decodeToString()).jsonObject
        val providerSettings = root.getValue("provider_settings").jsonObject
        val recommendedModels = root.getValue("recommended_model_defaults").jsonObject
        val chains = root.getValue("model_priority_chains").jsonObject
        val defaults = PresetRuntimeSettings()

        assertEquals(
            recommendedModels.getValue("generic_image").jsonPrimitive.content,
            DEFAULT_IMAGE_MODEL_ID,
        )
        // Image presets all track the default; there is no separate pin.
        assertNull(recommendedModels["accurate_image"])
        assertNull(recommendedModels["image_translate"])
        assertEquals(
            recommendedModels.getValue("image_ask").jsonPrimitive.content,
            DEFAULT_IMAGE_MODEL_ID,
        )
        assertEquals(
            recommendedModels.getValue("generic_text").jsonPrimitive.content,
            DEFAULT_TEXT_MODEL_ID,
        )
        assertEquals(
            recommendedModels.getValue("text_arena_fast").jsonPrimitive.content,
            PRESET_TEXT_ARENA_FAST_MODEL_ID,
        )
        assertEquals(
            recommendedModels.getValue("text_game").jsonPrimitive.content,
            PRESET_TEXT_GAME_MODEL_ID,
        )
        assertEquals(
            recommendedModels.getValue("search").jsonPrimitive.content,
            PRESET_SEARCH_MODEL_ID,
        )
        assertEquals(
            recommendedModels.getValue("audio_transcribe").jsonPrimitive.content,
            PRESET_AUDIO_TRANSCRIBE_MODEL_ID,
        )
        assertEquals(
            recommendedModels.getValue("audio_continuous").jsonPrimitive.content,
            PRESET_AUDIO_CONTINUOUS_MODEL_ID,
        )
        assertEquals(
            recommendedModels.getValue("audio_direct_translate").jsonPrimitive.content,
            PRESET_AUDIO_DIRECT_TRANSLATE_MODEL_ID,
        )
        assertEquals(
            recommendedModels.getValue("audio_offline_transcribe").jsonPrimitive.content,
            PRESET_AUDIO_OFFLINE_TRANSCRIBE_MODEL_ID,
        )
        assertEquals(
            providerSettings.getValue("use_groq").jsonPrimitive.boolean,
            defaults.providerSettings.useGroq,
        )
        assertEquals(
            providerSettings.getValue("use_gemini").jsonPrimitive.boolean,
            defaults.providerSettings.useGemini,
        )
        assertEquals(
            providerSettings.getValue("use_openrouter").jsonPrimitive.boolean,
            defaults.providerSettings.useOpenRouter,
        )
        assertEquals(
            providerSettings.getValue("use_ollama").jsonPrimitive.boolean,
            defaults.providerSettings.useOllama,
        )
        assertEquals(
            chains.getValue("image_to_text").jsonArray.map { it.jsonPrimitive.content },
            defaults.modelPriorityChains.imageToText,
        )
        assertEquals(
            chains.getValue("text_to_text").jsonArray.map { it.jsonPrimitive.content },
            defaults.modelPriorityChains.textToText,
        )
    }

    @Test
    fun imagePresetDefaultsKeepSpeedAndAccuracyPoliciesSeparate() {
        val byId = DefaultPresets.all.associateBy { it.id }

        listOf(
            "preset_translate",
            "preset_translate_auto_paste",
            "preset_translate_retranslate",
        ).forEach { id ->
            assertEquals(
                DEFAULT_IMAGE_MODEL_ID,
                byId.getValue(id).blocks.first().model,
            )
        }

        listOf(
            "preset_ocr",
            "preset_ocr_read",
            "preset_summarize",
            "preset_desc",
            "preset_ask_image",
        ).forEach { id ->
            assertEquals(DEFAULT_IMAGE_MODEL_ID, byId.getValue(id).blocks.first().model)
        }

        listOf(
            "preset_extract_table",
            "preset_fact_check",
            "preset_omniscient_god",
        ).forEach { id ->
            assertEquals(DEFAULT_IMAGE_MODEL_ID, byId.getValue(id).blocks.first().model)
        }
    }

    @Test
    fun retiredImagePresetAndWindowsOnlyHotkeyMatchSharedContract() {
        val root = json.parseToJsonElement(
            Files.readAllBytes(catalogFixturePath()).decodeToString(),
        ).jsonObject
        val retirements = root.getValue("retired_builtins").jsonArray.map { it.jsonObject }

        assertEquals(2, retirements.size)
        retirements.forEach { retirement ->
            val retiredId = retirement.getValue("preset_id").jsonPrimitive.content
            val replacementId = retirement.getValue("replacement_id").jsonPrimitive.content
            assertFalse(
                retirement.getValue("android_copies_hotkey_metadata").jsonPrimitive.boolean,
            )
            assertFalse(DefaultPresets.all.any { it.id == retiredId })
            assertTrue(DefaultPresets.all.single { it.id == replacementId }.hotkeys.isEmpty())
        }
        assertEquals(14, DefaultPresets.imagePresets.size)
    }

    private fun fixturePath(): Path {
        val candidates = listOf(
            Paths.get("..", "parity-fixtures", "preset-system", "retry-runtime.json"),
            Paths.get("..", "..", "parity-fixtures", "preset-system", "retry-runtime.json"),
            Paths.get("parity-fixtures", "preset-system", "retry-runtime.json"),
        )
        return candidates.firstOrNull(Files::exists)
            ?: error("Could not locate retry-runtime parity fixture.")
    }

    private fun catalogFixturePath(): Path {
        val candidates = listOf(
            Paths.get("..", "parity-fixtures", "preset-system", "catalog-overrides.json"),
            Paths.get("..", "..", "parity-fixtures", "preset-system", "catalog-overrides.json"),
            Paths.get("parity-fixtures", "preset-system", "catalog-overrides.json"),
        )
        return candidates.firstOrNull(Files::exists)
            ?: error("Could not locate catalog-overrides parity fixture.")
    }
}
