package dev.screengoated.toolbox.mobile.preset

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import okhttp3.OkHttpClient
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.Paths

class TextApiClientTest {
    private val json = Json { ignoreUnknownKeys = true }
    private val client = TextApiClient(OkHttpClient())

    @Test
    fun fixtureResolutionMatchesWindowsParityContract() {
        fixtureCases().forEach { case ->
            val resolved = client.debugResolveTextRequest(case.modelId)
            assertEquals(case.provider, resolved.provider.name)
            assertEquals(case.apiModel, resolved.apiModel)
            assertEquals(case.supportsSearch, resolved.supportsSearch)
            case.thinkingIncludeThoughts?.let { expected ->
                assertEquals(
                    expected,
                    resolved.geminiThinkingConfig?.get("includeThoughts"),
                )
            }
        }
    }

    @Test
    fun cerebrasRequestBodyUsesResolvedWindowsApiModel() {
        val payload = json.parseToJsonElement(
            client.debugBuildRequestBody(
                modelId = "cerebras_gpt_oss",
                prompt = "Translate to Vietnamese.",
                inputText = "Hello",
            ),
        ).jsonObject

        assertEquals(
            PresetModelCatalog.getById("cerebras_gpt_oss")!!.fullName,
            payload.getValue("model").jsonPrimitive.content,
        )
        assertTrue(payload.getValue("stream").jsonPrimitive.boolean)
    }

    @Test
    fun geminiRequestBodyCarriesWindowsThinkingConfigAndSearchRules() {
        val payload = json.parseToJsonElement(
            client.debugBuildRequestBody(
                modelId = "text_gemini_3_0_flash",
                prompt = "Summarize this.",
                inputText = "Hello",
            ),
        ).jsonObject

        val generationConfig = payload.getValue("generationConfig").jsonObject
        assertEquals(
            true,
            generationConfig.getValue("thinkingConfig").jsonObject
                .getValue("includeThoughts")
                .jsonPrimitive
                .boolean,
        )
        assertFalse(payload.containsKey("tools"))
    }

    @Test
    fun compoundMiniBodyUsesCompoundToolsContract() {
        val payload = json.parseToJsonElement(
            client.debugBuildRequestBody(
                modelId = "compound_mini",
                prompt = "Search this.",
                inputText = "Hello",
            ),
        ).jsonObject

        assertEquals("groq/compound-mini", payload.getValue("model").jsonPrimitive.content)
        assertFalse(payload.getValue("stream").jsonPrimitive.boolean)
        val tools = payload.getValue("compound_custom")
            .jsonObject
            .getValue("tools")
            .jsonObject
            .getValue("enabled_tools")
            .jsonArray
            .map { it.jsonPrimitive.content }
        assertEquals(listOf("web_search", "visit_website"), tools)
    }

    @Test
    fun geminiRequestRespectsStreamingToggle() {
        val payload = json.parseToJsonElement(
            client.debugBuildRequestBody(
                modelId = "text_gemini_3_0_flash",
                prompt = "Summarize this.",
                inputText = "Hello",
                streamingEnabled = false,
            ),
        ).jsonObject
        assertFalse(payload.getValue("stream").jsonPrimitive.boolean)
    }

    private fun fixtureCases(): List<FixtureCase> {
        val root = json.parseToJsonElement(Files.readAllBytes(fixturePath()).decodeToString()).jsonObject
        return root.getValue("cases").jsonArray.map { element ->
            val case = element.jsonObject
            FixtureCase(
                name = case.getValue("name").jsonPrimitive.content,
                modelId = case.getValue("model_id").jsonPrimitive.content,
                provider = case.getValue("provider").jsonPrimitive.content,
                apiModel = case.getValue("api_model").jsonPrimitive.content,
                supportsSearch = case.getValue("supports_search").jsonPrimitive.boolean,
                thinkingIncludeThoughts = case["thinking_include_thoughts"]?.jsonPrimitive?.booleanOrNull,
            )
        }
    }

    private fun fixturePath(): Path {
        val candidates = listOf(
            Paths.get("..", "parity-fixtures", "preset-system", "text-provider-routing.json"),
            Paths.get("..", "..", "parity-fixtures", "preset-system", "text-provider-routing.json"),
            Paths.get("parity-fixtures", "preset-system", "text-provider-routing.json"),
        )
        return candidates.firstOrNull(Files::exists)
            ?: error("Could not locate text-provider-routing parity fixture.")
    }

    private data class FixtureCase(
        val name: String,
        val modelId: String,
        val provider: String,
        val apiModel: String,
        val supportsSearch: Boolean,
        val thinkingIncludeThoughts: Boolean?,
    )
}
