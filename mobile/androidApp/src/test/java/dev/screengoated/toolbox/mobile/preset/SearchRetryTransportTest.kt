package dev.screengoated.toolbox.mobile.preset

import dev.screengoated.toolbox.mobile.AppToastBus
import dev.screengoated.toolbox.mobile.shared.preset.Preset
import dev.screengoated.toolbox.mobile.shared.preset.PresetInput
import dev.screengoated.toolbox.mobile.shared.preset.PresetType
import dev.screengoated.toolbox.mobile.shared.preset.imageBlock
import dev.screengoated.toolbox.mobile.shared.preset.textBlock
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.runBlocking
import okhttp3.OkHttpClient
import okhttp3.Protocol
import okhttp3.Response
import okhttp3.ResponseBody.Companion.toResponseBody
import okio.Buffer
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class SearchRetryTransportTest {
    @After
    fun clearModels() = PresetCustomModelRegistry.set(emptyList())

    @Test
    fun imageSearchFailsBeforeAnyProviderRequest() = runBlocking {
        var calls = 0
        val client = OkHttpClient.Builder().addInterceptor { chain ->
            calls++
            error("Unexpected provider request: ${chain.request().url}")
        }.build()
        val state = MutableStateFlow(PresetExecutionState(sessionId = "test", isExecuting = true))
        val executor = PresetGraphExecutor(
            textApiClient = TextApiClient(client), visionApiClient = VisionApiClient(client),
            apiKeys = { ApiKeys(geminiKey = "test") }, runtimeSettings = { PresetRuntimeSettings() },
            uiLanguage = { "en" }, executionState = state, toastBus = AppToastBus(),
        )
        val preset = Preset(
            id = "image-search", nameEn = "Search", nameVi = "Search", nameKo = "Search",
            presetType = PresetType.IMAGE,
            blocks = listOf(imageBlock("google-gemini-3-5-flash-lite-vision", "Search")
                .copy(searchEnabled = true, showOverlay = true, renderMode = "markdown")),
        )
        executor.executeGraph("test", preset, PresetInput.Image(byteArrayOf(1, 2, 3)))
        assertEquals(0, calls)
        assertTrue(state.value.resultWindows.single().isError)
        assertTrue(state.value.resultWindows.single().markdownText.contains("SEARCH_UNSUPPORTED:image"))
    }

    @Test
    fun graphRetriesKeepSearchIntentAfterPreflightAndProviderFailure() = runBlocking {
        for ((search, preflight) in listOf(false to true, true to true, true to false)) {
            val requests = mutableListOf<Pair<String, String>>()
            val client = OkHttpClient.Builder().addInterceptor { chain ->
                val buffer = Buffer()
                chain.request().body!!.writeTo(buffer)
                requests += chain.request().url.host to buffer.readUtf8()
                val google = chain.request().url.host.contains("googleapis")
                Response.Builder().request(chain.request()).protocol(Protocol.HTTP_1_1)
                    .code(if (google) 503 else 200).message(if (google) "Unavailable" else "OK")
                    .body((if (google) "temporary failure" else
                        "data: {\"choices\":[{\"delta\":{\"content\":\"done\"}}]}\n\ndata: [DONE]\n\n").toResponseBody()).build()
            }.build()
            val state = MutableStateFlow(PresetExecutionState(sessionId = "test", isExecuting = true))
            val executor = PresetGraphExecutor(
                textApiClient = TextApiClient(client), visionApiClient = VisionApiClient(client),
                apiKeys = { ApiKeys(geminiKey = if (preflight) "" else "test", groqKey = "test") },
                runtimeSettings = { PresetRuntimeSettings(
                    modelPriorityChains = PresetModelPriorityChains(textToText = listOf(
                        "groq-qwen-3-8-27b-text", "groq-gpt-oss-20b-text",
                    )),
                    adaptiveModelPriority = PresetAdaptiveModelPriority(textToText = false),
                ) },
                uiLanguage = { "en" }, executionState = state, toastBus = AppToastBus(),
            )
            val preset = Preset(
                id = "search-retry", nameEn = "Test", nameVi = "Test", nameKo = "Test",
                presetType = PresetType.TEXT_INPUT,
                blocks = listOf(textBlock("google-gemini-3-1-flash-lite-text", "Answer")
                    .copy(searchEnabled = search, showOverlay = true, renderMode = "markdown")),
            )
            executor.executeGraph("test", preset, PresetInput.Text("question"))
            val groq = requests.single { it.first.contains("groq") }.second
            assertTrue(groq.contains(if (search) "openai/gpt-oss-20b" else "qwen"))
            assertEquals(search, groq.contains("browser_search"))
            assertFalse(state.value.resultWindows.single().isError)
            clearPresetModelCircuitsForTest()
        }
    }

    @Test
    fun customSearchEligibilityMatchesProviderPayloadAndDeniesUnsupportedAdapters() = runBlocking {
        val models = listOf(
            CustomPresetModelDefinition("custom-google", PresetModelProvider.GOOGLE, "Google", "custom-google", supportsSearch = true),
            CustomPresetModelDefinition("custom-groq", PresetModelProvider.GROQ, "Groq", "custom-groq", supportsSearch = true),
            CustomPresetModelDefinition("custom-openrouter", PresetModelProvider.OPENROUTER, "OpenRouter", "custom-openrouter", supportsSearch = true),
            CustomPresetModelDefinition("custom-disabled", PresetModelProvider.GOOGLE, "Google", "custom-disabled", supportsSearch = false),
        )
        PresetCustomModelRegistry.set(models)
        val bodies = mutableListOf<String>()
        val client = OkHttpClient.Builder().addInterceptor { chain ->
            val buffer = Buffer()
            chain.request().body!!.writeTo(buffer)
            bodies += buffer.readUtf8()
            val body = if (chain.request().url.host.contains("googleapis")) {
                """{"candidates":[{"content":{"parts":[{"text":"done"}]},"finishReason":"STOP"}]}"""
            } else {
                """{"choices":[{"message":{"content":"done"},"finish_reason":"stop"}]}"""
            }
            Response.Builder().request(chain.request()).protocol(Protocol.HTTP_1_1).code(200).message("OK")
                .body(body.toResponseBody()).build()
        }.build()
        val text = TextApiClient(client)
        for (id in listOf("custom-openrouter", "custom-disabled")) {
            assertFalse(PresetModelCatalog.supportsTextSearch(requireNotNull(PresetModelCatalog.getById(id))))
            val result = text.executeStreaming(id, "Search", "private", ApiKeys(geminiKey = "test", openRouterKey = "test"), "en", null, {}, false, true)
            assertTrue(result.isFailure)
        }
        assertTrue(bodies.isEmpty())
        for (id in listOf("custom-google", "custom-groq")) {
            assertTrue(PresetModelCatalog.supportsTextSearch(requireNotNull(PresetModelCatalog.getById(id))))
            val result = text.executeStreaming(id, "Search", "private", ApiKeys(geminiKey = "test", groqKey = "test"), "en", null, {}, false, true)
            assertTrue(result.isSuccess)
        }
        assertEquals(2, bodies.size)
        assertTrue(bodies[0].contains("google_search"))
        assertTrue(bodies[1].contains("browser_search"))
    }
}
