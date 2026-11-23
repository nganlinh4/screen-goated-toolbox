package dev.screengoated.toolbox.mobile.preset

import dev.screengoated.toolbox.mobile.shared.preset.DEFAULT_IMAGE_MODEL_ID
import dev.screengoated.toolbox.mobile.shared.preset.DEFAULT_TEXT_MODEL_ID
import dev.screengoated.toolbox.mobile.shared.preset.DefaultPresets
import dev.screengoated.toolbox.mobile.shared.preset.PRESET_AUDIO_CONTINUOUS_MODEL_ID
import dev.screengoated.toolbox.mobile.shared.preset.PRESET_AUDIO_DIRECT_TRANSLATE_MODEL_ID
import dev.screengoated.toolbox.mobile.shared.preset.PRESET_AUDIO_DEVICE_CONTINUOUS_MODEL_ID
import dev.screengoated.toolbox.mobile.shared.preset.PRESET_AUDIO_TRANSCRIBE_MODEL_ID
import dev.screengoated.toolbox.mobile.shared.preset.PRESET_SEARCH_MODEL_ID
import dev.screengoated.toolbox.mobile.shared.preset.PRESET_TEXT_ARENA_FAST_MODEL_ID
import dev.screengoated.toolbox.mobile.shared.preset.PRESET_TEXT_GAME_MODEL_ID
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.contentOrNull
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

    @Test
    fun movedChosenRowMatchesWindowsAttemptOrder() {
        val fixtureText = Files.readAllBytes(fixturePath("preset-attempt-order.json")).decodeToString()
        val fixture = json.parseToJsonElement(fixtureText).jsonObject
        fixture.getValue("cases").jsonArray.forEach { entry ->
            val case = entry.jsonObject
            assertEquals(
                case.getValue("attempts").jsonArray.map { it.jsonPrimitive.content },
                expandChosenModel(
                    case.getValue("rows").jsonArray.map { it.jsonPrimitive.content },
                    case.getValue("selected").jsonPrimitive.content,
                ),
            )
        }
        val live = fixture.getValue("live").jsonObject
        val merged = mergePriorityRows(
            live.getValue("rows").jsonArray.map { it.jsonPrimitive.content },
            live.getValue("offered").jsonArray.map { it.jsonPrimitive.content },
            PresetLiveModelOverrides(), 2,
        ) { id -> AdaptiveCandidateRank(4, if (id == "live-auto") 100 else 2000) }
        assertEquals(live.getValue("order").jsonArray.map { it.jsonPrimitive.content }, merged)
        val case = fixture.getValue("runtime").jsonObject
        val selected = case.getValue("selected").jsonPrimitive.content
        val first = case.getValue("first").jsonPrimitive.content
        val last = case.getValue("last").jsonPrimitive.content
        val (rows, chosenAfter) = splitPriorityOrder(case.getValue("rows").jsonArray.map { it.jsonPrimitive.content })
        val settings = PresetRuntimeSettings(
            providerSettings = PresetProviderSettings(useGroq = true, useGemini = true, useOpenRouter = false),
            modelPriorityChains = PresetModelPriorityChains(textToText = rows, textChosenAfter = chosenAfter),
            adaptiveModelPriority = PresetAdaptiveModelPriority(textToText = false),
        )
        val keys = ApiKeys(groqKey = "test-groq", geminiKey = "test-gemini")
        val kind = PresetRetryChainKind.TEXT_TO_TEXT
        val order = PresetAttemptOrder(kind, selected, false, keys, settings)
        assertEquals(first, order.initialId())
        assertEquals(selected, PresetAttemptOrder(kind, selected, true, keys, settings).initialId())
        assertEquals(selected, order.next(first, listOf(first), emptySet())?.id)
        assertEquals(last, order.next(selected, listOf(first), emptySet())?.id)
    }

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
        assertTrue(shouldAdvanceRetryChain("EMPTY_MODEL_RESPONSE:model/name"))
        assertTrue(
            shouldAdvanceRetryChain(
                "PROVIDER_TRANSPORT_UNAVAILABLE: TLS handshake failed",
            ),
        )
        assertFalse(shouldAdvanceRetryChain("request failed with 200"))
    }

    @Test
    fun blocksProviderForAuthAvailabilityAndPreResponseTransportErrors() {
        assertTrue(shouldBlockRetryProvider("NO_API_KEY:groq"))
        assertTrue(shouldBlockRetryProvider("INVALID_API_KEY"))
        assertTrue(shouldBlockRetryProvider("PROVIDER_DISABLED:google"))
        assertTrue(shouldBlockRetryProvider("PROVIDER_NOT_READY:gemini-live"))
        assertTrue(
            shouldBlockRetryProvider(
                "PROVIDER_TRANSPORT_UNAVAILABLE: failed to connect to api.example.test",
            ),
        )
        assertTrue(shouldBlockRetryProvider("request failed with status code 403"))
        assertFalse(shouldBlockRetryProvider("request failed with status code 404"))
        assertFalse(shouldBlockRetryProvider("timeout waiting for response headers"))
        assertFalse(shouldBlockRetryProvider("timeout reading response body"))
        assertFalse(shouldBlockRetryProvider("whole attempt timeout"))
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
    fun initialFailureCooldownsMatchSharedFixture() {
        val policy = json.parseToJsonElement(Files.readAllBytes(fixturePath()).decodeToString())
            .jsonObject.getValue("retry_circuit").jsonObject
        val duration = policy.getValue("first_timeout_open_seconds").jsonPrimitive.content.toLong() * 1_000L
        assertEquals(duration, policy.getValue("invalid_response_open_seconds").jsonPrimitive.content.toLong() * 1_000L)
        policy.getValue("initial_failure_cases").jsonArray.forEachIndexed { index, value ->
            val case = value.jsonObject
            val model = "shared-initial-failure-$index"
            recordPresetModelFailureAt(model, case.getValue("error").jsonPrimitive.content, 1_000L)
            val reason = case.getValue("reason").jsonPrimitive.content
            if (reason.isEmpty()) {
                assertNull(claimPresetModelAttemptAt(model, 1_000L))
            } else {
                assertEquals("$reason:$model:15s", claimPresetModelAttemptAt(model, 1_000L))
                assertNotNull(claimPresetModelAttemptAt(model, 1_000L + duration - 1L))
                assertNull(claimPresetModelAttemptAt(model, 1_000L + duration))
                assertEquals("MODEL_COOLDOWN_PROBE_IN_FLIGHT:$model", claimPresetModelAttemptAt(model, 1_000L + duration))
                recordPresetModelFailureAt(model, "PROVIDER_RESPONSE_INVALID:Provider completion token limit reached", 1_000L + duration)
                assertNull(claimPresetModelAttemptAt(model, 1_000L + duration))
            }
            recordPresetModelSuccess(model)
        }
    }

    @Test
    fun timeoutBackoffIsGradualCappedAndResetsAfterSuccess() {
        val policy = json.parseToJsonElement(Files.readAllBytes(fixturePath()).decodeToString())
            .jsonObject.getValue("retry_circuit").jsonObject
        val durations = policy.getValue("consecutive_timeout_cooldowns_seconds").jsonArray
        val modelId = "timeout-circuit-model"
        var now = 1_000L
        repeat(2) {
            durations.forEach { value ->
                val seconds = value.jsonPrimitive.content.toLong()
                recordPresetModelFailureAt(modelId, "request timed out", now)
                assertTrue(claimPresetModelAttemptAt(modelId, now)?.endsWith(":${seconds}s") == true)
                recordPresetModelFailureAt(modelId, "timeout", now + 1_000L)
                now += seconds * 1_000L
                assertNotNull(claimPresetModelAttemptAt(modelId, now - 1L))
                assertNull(claimPresetModelAttemptAt(modelId, now))
                assertEquals("MODEL_COOLDOWN_PROBE_IN_FLIGHT:$modelId", claimPresetModelAttemptAt(modelId, now))
                releasePresetModelProbeAt(modelId, now)
                assertNull(claimPresetModelAttemptAt(modelId, now))
            }
            recordPresetModelSuccess(modelId)
            assertNull(claimPresetModelAttemptAt(modelId, now))
        }
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
    fun staleAttemptCannotReleaseOrCompleteAnotherAttemptsProbe() {
        val modelId = "owned-half-open-model"
        val oldAttempt = PresetModelAttempt(modelId)
        val probe = PresetModelAttempt(modelId)
        val nextAttempt = PresetModelAttempt(modelId)

        assertNull(claimPresetModelAttemptAt(modelId, 1_000L, oldAttempt))
        recordPresetModelFailureAt(modelId, "HTTP 429 retry-after: 5", 1_000L)
        assertNull(claimPresetModelAttemptAt(modelId, 6_000L, probe))

        releasePresetModelProbeAt(modelId, 6_000L, oldAttempt)
        recordPresetModelFailureAt(modelId, "request timed out", 6_000L, oldAttempt)
        recordPresetModelSuccess(oldAttempt)
        assertEquals("MODEL_COOLDOWN_PROBE_IN_FLIGHT:$modelId", claimPresetModelAttemptAt(modelId, 6_000L, nextAttempt))

        releasePresetModelProbeAt(modelId, 6_000L, probe)
        assertNull(claimPresetModelAttemptAt(modelId, 6_000L, nextAttempt))
        recordPresetModelSuccess(nextAttempt)
        assertNull(presetModelCircuitSkipReasonAt(modelId, 6_000L))
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
            searchEnabled = false,
        )

        assertNotNull(next)
        assertEquals(
            GeneratedPresetModelCatalogData.modelPriorityChains.textToText.first(),
            next?.id,
        )
    }

    @Test
    fun retrySelectionUsesRequestSearchIntentFromSharedFixture() {
        val search = json.parseToJsonElement(Files.readAllBytes(fixturePath()).decodeToString())
            .jsonObject.getValue("search_retry").jsonObject
        val chain = search.getValue("candidate_order").jsonArray.map { it.jsonPrimitive.content }
        val settings = PresetRuntimeSettings(
            modelPriorityChains = PresetModelPriorityChains(textToText = chain),
            adaptiveModelPriority = PresetAdaptiveModelPriority(textToText = false),
        )
        val keys = ApiKeys(groqKey = "test", geminiKey = "test")
        search.getValue("cases").jsonArray.forEach { raw ->
            val case = raw.jsonObject
            val next = resolveNextConfiguredRetryModel(
                currentModelId = case.getValue("current_model_id").jsonPrimitive.content,
                failedModelIds = case.getValue("failed_model_ids").jsonArray.map { it.jsonPrimitive.content },
                blockedProviders = emptySet(),
                chainKind = PresetRetryChainKind.TEXT_TO_TEXT,
                apiKeys = keys,
                settings = settings,
                searchEnabled = case.getValue("search_enabled").jsonPrimitive.boolean,
            )
            assertEquals(case.getValue("name").jsonPrimitive.content,
                case.getValue("expected_configured_model_id").jsonPrimitive.contentOrNull, next?.id)
            if (next != null) {
                val payload = json.parseToJsonElement(
                    TextApiClient(okhttp3.OkHttpClient()).debugBuildRequestBody(
                        next.id, "prompt", "input", searchEnabled = case.getValue("search_enabled").jsonPrimitive.boolean,
                    ),
                ).jsonObject
                assertEquals(case.getValue("expected_search_tool").jsonPrimitive.boolean, payload.containsKey("tools"))
            }
        }
        search.getValue("compatibility_cases").jsonArray.forEach { raw ->
            val case = raw.jsonObject
            val candidate = PresetModelDescriptor(
                id = "fixture-model", provider = PresetModelProvider.GOOGLE,
                fullName = "fixture-model", modelType = PresetModelType.TEXT, displayName = "Fixture",
                supportsSearchOverride = case.getValue("candidate_supports_search").jsonPrimitive.boolean,
                searchToolEnabledByDefault = case.getValue("candidate_search_tool_enabled_by_default").jsonPrimitive.boolean,
            )
            assertEquals(case.getValue("name").jsonPrimitive.content,
                case.getValue("expected_eligible").jsonPrimitive.boolean,
                retrySearchCompatible(candidate, case.getValue("search_enabled").jsonPrimitive.boolean))
            assertEquals(case.getValue("name").jsonPrimitive.content,
                case.getValue("search_enabled").jsonPrimitive.boolean || !candidate.searchToolEnabledByDefault,
                runCatching { requireExplicitSearchOptIn(candidate, case.getValue("search_enabled").jsonPrimitive.boolean) }.isSuccess)
        }
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
            recommendedModels.getValue("audio_device_continuous").jsonPrimitive.content,
            PRESET_AUDIO_DEVICE_CONTINUOUS_MODEL_ID,
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

    private fun fixturePath(name: String = "retry-runtime.json"): Path {
        val candidates = listOf(
            Paths.get("..", "parity-fixtures", "preset-system", name),
            Paths.get("..", "..", "parity-fixtures", "preset-system", name),
            Paths.get("parity-fixtures", "preset-system", name),
        )
        return candidates.firstOrNull(Files::exists)
            ?: error("Could not locate $name parity fixture.")
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
