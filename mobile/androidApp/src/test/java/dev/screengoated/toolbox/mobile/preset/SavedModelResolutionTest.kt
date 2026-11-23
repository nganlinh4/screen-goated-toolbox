package dev.screengoated.toolbox.mobile.preset

import dev.screengoated.toolbox.mobile.AppToastBus
import dev.screengoated.toolbox.mobile.shared.preset.Preset
import dev.screengoated.toolbox.mobile.shared.preset.PresetInput
import dev.screengoated.toolbox.mobile.shared.preset.PresetType
import dev.screengoated.toolbox.mobile.shared.preset.audioBlock
import dev.screengoated.toolbox.mobile.shared.preset.imageBlock
import dev.screengoated.toolbox.mobile.shared.preset.textBlock
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
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
import java.io.File
import java.util.concurrent.atomic.AtomicInteger

@OptIn(ExperimentalCoroutinesApi::class)
class SavedModelResolutionTest {
    private val json = Json { ignoreUnknownKeys = true }

    @After
    fun clearDiscovery() {
        PresetCustomModelRegistry.set(emptyList())
        PresetModelFeed.publish(null)
    }

    @Test
    fun blankSavedPinsRemainUnchangedAndNeverDispatch() = runTest {
        for (kind in listOf("text", "image", "audio")) {
            for (id in listOf("", "  ")) {
                val selected = preset(id, kind)
                val store = MemoryStore(StoredPresetOverrides(customPresets = mapOf(selected.id to selected)))
                val calls = AtomicInteger()
                val repo = repository(store, calls, StandardTestDispatcher(testScheduler))
                val resolved = requireNotNull(repo.getResolvedPreset(selected.id))
                assertEquals(id, resolved.preset.blocks.single().model)
                assertEquals(PresetPlaceholderReason.MODEL_UNAVAILABLE, resolved.executionCapability.reason)
                repo.executePreset(resolved.preset, PresetInput.Text("sensitive text"))
                runCurrent()
                assertFalse(repo.executionState.value.isExecuting)
                assertEquals(0, calls.get())
                repo.updateBuiltInOverride(selected.id) { it.copy(isFavorite = true) }
                assertEquals(id, store.load().customPresets.getValue(selected.id).blocks.single().model)
            }
        }
    }

    @Test
    fun sharedSavedModelCasesPreservePinsAndFailClosed() = runTest {
        val cases = fixture().getValue("pinned_cases").jsonArray
        for (case in cases) {
            val item = case.jsonObject
            val id = item.getValue("model_id").jsonPrimitive.content
            val kind = item.getValue("block_type").jsonPrimitive.content
            val resolvedType = item.getValue("resolved_model_type").jsonPrimitive.contentOrNull
            PresetCustomModelRegistry.set(resolvedType?.let {
                listOf(CustomPresetModelDefinition(
                    id = id,
                    provider = PresetModelProvider.OLLAMA,
                    displayName = "Example",
                    fullName = "example-model",
                    modelType = if (it == "vision") PresetModelType.VISION else PresetModelType.TEXT,
                ))
            }.orEmpty())
            val preset = preset(id, kind)
            val store = MemoryStore(StoredPresetOverrides(customPresets = mapOf(preset.id to preset)))
            val calls = AtomicInteger()
            val repo = repository(store, calls, StandardTestDispatcher(testScheduler))
            val actual = requireNotNull(repo.getResolvedPreset(preset.id))
            assertEquals(item.getValue("name").jsonPrimitive.content,
                item.getValue("expected_saved_id").jsonPrimitive.content, actual.preset.blocks.single().model)
            val expectedDispatch = item.getValue("expected_action").jsonPrimitive.content == "dispatch_selected"
            assertEquals(expectedDispatch, actual.executionCapability.supported)
            if (!expectedDispatch) {
                repo.executePreset(actual.preset, PresetInput.Text("sensitive text"))
                runCurrent()
                assertFalse(repo.executionState.value.isExecuting)
                assertTrue(repo.executionState.value.error!!.contains("unavailable"))
                assertEquals(0, calls.get())
            }
            repo.updateBuiltInOverride(preset.id) { it.copy(isFavorite = true) }
            assertEquals(id, store.load().customPresets.getValue(preset.id).blocks.single().model)
        }
    }

    @Test
    fun unrelatedBuiltInEditRetainsUnknownModelOverride() = runTest {
        val original = dev.screengoated.toolbox.mobile.shared.preset.DefaultPresets.all
            .first { it.presetType == PresetType.TEXT_INPUT && it.blocks.any { block -> block.blockType == dev.screengoated.toolbox.mobile.shared.preset.BlockType.TEXT } }
        val selected = original.blocks.map { block ->
            if (block.blockType == dev.screengoated.toolbox.mobile.shared.preset.BlockType.TEXT) block.copy(model = "unknown-saved-text") else block
        }
        val store = MemoryStore(StoredPresetOverrides(builtInOverrides = mapOf(original.id to PresetOverride(blocks = selected))))
        val repo = repository(store, AtomicInteger(), StandardTestDispatcher(testScheduler))
        repo.updateBuiltInOverride(original.id) { it.copy(isFavorite = !it.isFavorite) }
        val saved = requireNotNull(store.load().builtInOverrides[original.id]?.blocks)
        assertTrue(saved.any { it.model == "unknown-saved-text" })
        assertEquals(PresetPlaceholderReason.MODEL_UNAVAILABLE, repo.getResolvedPreset(original.id)?.executionCapability?.reason)
    }

    @Test
    fun savedChainsKeepUnknownRowsAcrossSerialization() {
        for (case in fixture().getValue("chain_cases").jsonArray) {
            val item = case.jsonObject
            val input = item.getValue("authored").jsonArray.map { it.jsonPrimitive.content }
            val expected = item.getValue("expected_saved").jsonArray.map { it.jsonPrimitive.content }
            val settings = PresetRuntimeSettings(modelPriorityChains = PresetModelPriorityChains(textToText = input))
                .normalizeModelPriorityChains()
            val reloaded = json.decodeFromString<PresetRuntimeSettings>(json.encodeToString(PresetRuntimeSettings.serializer(), settings))
            assertEquals(expected, reloaded.modelPriorityChains.textToText)
        }
    }

    @Test
    fun feedDiscoveryRefreshesCapabilityAndPreservesTheSavedId() = runTest {
        val endpoint = "example/provider-model"
        val id = discoveredModelId("nvidia", endpoint)
        val selected = preset(id, "text")
        val store = MemoryStore(StoredPresetOverrides(customPresets = mapOf(selected.id to selected)))
        val repo = repository(store, AtomicInteger(), StandardTestDispatcher(testScheduler))
        runCurrent()
        assertFalse(requireNotNull(repo.getResolvedPreset(selected.id)).executionCapability.supported)
        PresetModelFeed.publish(AvailabilityFeed(2, 1, 1, "nvidia", "2026-01-01T00:00:00Z", listOf(
            FeedModel(endpoint, null, "text", 100, 1.0, 1),
        )))
        runCurrent()
        assertTrue(requireNotNull(repo.getResolvedPreset(selected.id)).executionCapability.supported)
        assertTrue(repo.catalogState.value.findPreset(selected.id)!!.executionCapability.supported)
        assertEquals(id, store.load().customPresets.getValue(selected.id).blocks.single().model)
        PresetModelFeed.publish(null)
        runCurrent()
        assertFalse(repo.catalogState.value.findPreset(selected.id)!!.executionCapability.supported)
        assertEquals(id, store.load().customPresets.getValue(selected.id).blocks.single().model)
    }

    @Test
    fun rediscoveredModelIsDispatchedToItsOwnProvider() = runBlocking {
        val id = "test-dynamic-model"
        PresetCustomModelRegistry.set(listOf(CustomPresetModelDefinition(
            id = id, provider = PresetModelProvider.OLLAMA, displayName = "Example",
            fullName = "selected-endpoint", modelType = PresetModelType.TEXT,
        )))
        val requests = mutableListOf<Pair<String, String>>()
        val client = OkHttpClient.Builder().addInterceptor { chain ->
            val body = Buffer()
            chain.request().body!!.writeTo(body)
            requests.add(chain.request().url.toString() to body.readUtf8())
            Response.Builder().request(chain.request()).protocol(Protocol.HTTP_1_1).code(200).message("OK")
                .body("""{"response":"done","done":true}""".toResponseBody()).build()
        }.build()
        val executor = PresetGraphExecutor(
            textApiClient = TextApiClient(client), visionApiClient = VisionApiClient(client),
            apiKeys = { ApiKeys(groqKey = "test", ollamaBaseUrl = "http://localhost:11434") },
            runtimeSettings = { PresetRuntimeSettings(providerSettings = PresetProviderSettings(useOllama = true)) },
            uiLanguage = { "en" }, executionState = MutableStateFlow(PresetExecutionState(sessionId = "test-session", isExecuting = true)), toastBus = AppToastBus(),
        )
        val selected = preset(id, "text").let { it.copy(blocks = it.blocks.map { block -> block.copy(showOverlay = false) }) }
        executor.executeGraph("test-session", selected, PresetInput.Text("sensitive text"))
        assertEquals(1, requests.size)
        assertEquals("http://localhost:11434/api/generate", requests.single().first)
        assertTrue(requests.single().second.contains("selected-endpoint"))
    }

    private fun fixture() = json.parseToJsonElement(
        generateSequence(File(requireNotNull(System.getProperty("user.dir"))).absoluteFile) { it.parentFile }
            .map { File(it, "parity-fixtures/preset-system/saved-model-resolution.json") }
            .first(File::isFile).readText(),
    ).jsonObject

    private fun preset(id: String, kind: String): Preset = Preset(
        id = "saved-model-test", nameEn = "Test", nameVi = "Test", nameKo = "Test",
        presetType = when (kind) { "image" -> PresetType.IMAGE; "audio" -> PresetType.MIC; else -> PresetType.TEXT_INPUT },
        blocks = listOf(when (kind) { "image" -> imageBlock(id, ""); "audio" -> audioBlock(id); else -> textBlock(id, "") }),
    )

    private fun repository(store: PresetOverrideStore, calls: AtomicInteger, dispatcher: CoroutineDispatcher): PresetRepository {
        val client = OkHttpClient.Builder().addInterceptor { chain ->
            calls.incrementAndGet()
            chain.proceed(chain.request())
        }.build()
        return PresetRepository(
            textApiClient = TextApiClient(client), visionApiClient = VisionApiClient(client),
            apiKeys = { ApiKeys(groqKey = "test") }, runtimeSettings = { PresetRuntimeSettings() },
            uiLanguage = { "en" }, overrideStore = store, toastBus = AppToastBus(), mainDispatcher = dispatcher,
        )
    }

    private class MemoryStore(private var value: StoredPresetOverrides) : PresetOverrideStore {
        override fun load() = value
        override fun save(overrides: StoredPresetOverrides) { value = overrides }
    }
}
