package dev.screengoated.toolbox.mobile.preset

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.Paths

class PresetRetryChainTest {
    private val json = Json { ignoreUnknownKeys = true }

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
                modelId = "text_gemini_3_0_flash",
                provider = PresetModelProvider.GOOGLE,
                apiKeys = ApiKeys(),
                blockedProviders = emptySet(),
                settings = PresetRuntimeSettings(),
            ),
        )
    }

    @Test
    fun retryResolutionUsesWindowsDefaultChainFirst() {
        val next = resolveNextRetryModel(
            currentModelId = "cerebras_gpt_oss",
            failedModelIds = listOf("cerebras_gpt_oss"),
            blockedProviders = emptySet(),
            chainKind = PresetRetryChainKind.TEXT_TO_TEXT,
            apiKeys = ApiKeys(
                geminiKey = "g",
                cerebrasKey = "c",
                groqKey = "r",
                openRouterKey = "o",
                ollamaBaseUrl = "http://localhost:11434",
            ),
            settings = PresetRuntimeSettings(),
        )

        assertNotNull(next)
        assertEquals("text_accurate_kimi", next?.id)
    }

    @Test
    fun disabledProviderIsSkippedLikeWindowsConfig() {
        assertEquals(
            "PROVIDER_DISABLED:google",
            preflightSkipReason(
                modelId = "text_gemini_3_0_flash",
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
        val chains = root.getValue("model_priority_chains").jsonObject
        val defaults = PresetRuntimeSettings()

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
            providerSettings.getValue("use_cerebras").jsonPrimitive.boolean,
            defaults.providerSettings.useCerebras,
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

    private fun fixturePath(): Path {
        val candidates = listOf(
            Paths.get("..", "parity-fixtures", "preset-system", "retry-runtime.json"),
            Paths.get("..", "..", "parity-fixtures", "preset-system", "retry-runtime.json"),
            Paths.get("parity-fixtures", "preset-system", "retry-runtime.json"),
        )
        return candidates.firstOrNull(Files::exists)
            ?: error("Could not locate retry-runtime parity fixture.")
    }
}
