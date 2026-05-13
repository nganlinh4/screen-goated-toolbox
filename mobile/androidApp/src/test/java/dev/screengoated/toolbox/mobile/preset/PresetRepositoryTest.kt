package dev.screengoated.toolbox.mobile.preset

import dev.screengoated.toolbox.mobile.AppToastBus
import dev.screengoated.toolbox.mobile.preset.PresetPlaceholderReason
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.float
import kotlinx.serialization.json.int
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

@OptIn(ExperimentalCoroutinesApi::class)
class PresetRepositoryTest {
    private val json = Json { ignoreUnknownKeys = true }
    private val dispatcher = StandardTestDispatcher()

    @Test
    fun fixtureOverrideMergeAndRestoreMatchRepository() {
        val cases = fixtureCases()
        val mergeCase = cases.first { it.name == "favorite_override_merges_onto_builtin" }
        val restoreCase = cases.first { it.name == "restore_default_removes_override" }

        val mergeStore = InMemoryPresetOverrideStore(
            StoredPresetOverrides(
                builtInOverrides = mapOf(
                    mergeCase.presetId to mergeCase.override!!.toPresetOverride(),
                ),
            ),
        )
        val mergeRepository = createRepository(mergeStore)
        val mergedPreset = requireNotNull(mergeRepository.getResolvedPreset(mergeCase.presetId)).preset
        assertTrue(mergedPreset.isFavorite)

        val restoreStore = InMemoryPresetOverrideStore(
            StoredPresetOverrides(
                builtInOverrides = mapOf(
                    restoreCase.presetId to restoreCase.override!!.toPresetOverride(),
                ),
            ),
        )
        val restoreRepository = createRepository(restoreStore)
        restoreRepository.restoreBuiltInPreset(restoreCase.presetId)
        val restoredPreset = requireNotNull(
            restoreRepository.getResolvedPreset(restoreCase.presetId),
        ).preset

        assertFalse(restoredPreset.isFavorite)
        assertEquals("fixed", restoredPreset.promptMode)
        assertFalse(
            restoreStore.load().builtInOverrides.containsKey(restoreCase.presetId),
        )
    }

    @Test
    fun fixtureExecutionCapabilitiesMatchRepository() {
        val repository = createRepository(InMemoryPresetOverrideStore())

        fixtureCases()
            .filter { it.expectedExecution != null }
            .forEach { case ->
                val resolved = requireNotNull(repository.getResolvedPreset(case.presetId))
                assertEquals(
                    case.expectedExecution!!.supported,
                    resolved.executionCapability.supported,
                )
                assertEquals(
                    case.expectedExecution.reason,
                    resolved.executionCapability.reason?.name,
                )
            }
    }

    @Test
    fun favoriteTogglePersistsAsBuiltInOverride() {
        val store = InMemoryPresetOverrideStore()
        val repository = createRepository(store)

        repository.toggleFavorite("preset_translate")

        val resolved = requireNotNull(repository.getResolvedPreset("preset_translate"))
        assertTrue(resolved.preset.isFavorite)
        assertTrue(resolved.hasOverride)
        assertEquals(
            true,
            store.load().builtInOverrides["preset_translate"]?.isFavorite,
        )
    }

    @Test
    fun windowGeometryOverridePersistsAsBuiltInOverride() {
        val store = InMemoryPresetOverrideStore()
        val repository = createRepository(store)

        repository.updateBuiltInOverride("preset_ask_ai") { preset ->
            preset.copy(
                windowGeometry = dev.screengoated.toolbox.mobile.shared.preset.WindowGeometry(
                    x = 120,
                    y = 180,
                    width = 420,
                    height = 320,
                ),
            )
        }

        val resolved = requireNotNull(repository.getResolvedPreset("preset_ask_ai")).preset
        assertEquals(120, resolved.windowGeometry?.x)
        assertEquals(180, resolved.windowGeometry?.y)
        assertEquals(420, store.load().builtInOverrides["preset_ask_ai"]?.windowGeometry?.width)
    }

    @Test
    fun htmlOutputTextPresetIsExecutable() {
        val repository = createRepository(InMemoryPresetOverrideStore())

        val resolved = requireNotNull(repository.getResolvedPreset("preset_make_game"))

        assertTrue(resolved.executionCapability.supported)
        assertFalse(resolved.placeholderReasons.contains(PresetPlaceholderReason.HTML_RESULT_NOT_READY))
    }

    @Test
    fun quickNotePresetIsSupported() {
        val repository = createRepository(InMemoryPresetOverrideStore())

        val resolved = requireNotNull(repository.getResolvedPreset("preset_quick_note"))

        assertTrue(resolved.executionCapability.supported)
        assertFalse(resolved.placeholderReasons.contains(PresetPlaceholderReason.TEXT_INPUT_OVERLAY_NOT_READY))
    }

    @Test
    fun hangImagePresetIsNoLongerUpcoming() {
        val repository = createRepository(InMemoryPresetOverrideStore())

        val resolved = requireNotNull(repository.getResolvedPreset("preset_hang_image"))

        assertFalse(resolved.preset.isUpcoming)
        assertTrue(resolved.executionCapability.supported)
    }

    @Test
    fun micAudioPresetIsSupported() {
        val repository = createRepository(InMemoryPresetOverrideStore())

        val resolved = requireNotNull(repository.getResolvedPreset("preset_transcribe"))

        assertTrue(resolved.executionCapability.supported)
        assertFalse(resolved.placeholderReasons.contains(PresetPlaceholderReason.AUDIO_CAPTURE_NOT_READY))
        assertEquals(
            "Transcribe the audio into text. Output ONLY the transcript.",
            resolved.preset.blocks.first().prompt,
        )
    }

    @Test
    fun realtimeAudioPresetIsSupported() {
        val repository = createRepository(InMemoryPresetOverrideStore())

        val resolved = requireNotNull(repository.getResolvedPreset("preset_realtime_audio_translate"))

        assertTrue(resolved.executionCapability.supported)
        assertFalse(resolved.placeholderReasons.contains(PresetPlaceholderReason.REALTIME_AUDIO_NOT_READY))
    }

    @Test
    fun deviceAudioBuiltInsKeepWindowsSourceAndRealtimeFlags() {
        val realtimePreset = requireNotNull(
            DefaultPresetLookup.byId("preset_realtime_audio_translate"),
        )
        val deviceRecordPreset = requireNotNull(
            DefaultPresetLookup.byId("preset_record_device"),
        )

        assertEquals("device", realtimePreset.audioSource)
        assertEquals("realtime", realtimePreset.audioProcessingMode)
        assertEquals("device", deviceRecordPreset.audioSource)
    }

    @Test
    fun audioRuntimeFixtureMatchesAndroidAudioDefaultsAndExplicitGaps() {
        val fixture = audioRuntimeFixture()
        val repository = createRepository(InMemoryPresetOverrideStore())
        val thresholds = fixture.getValue("auto_stop_thresholds").jsonObject

        assertEquals(0.001f, thresholds.getValue("warmup_rms").jsonPrimitive.float)
        assertEquals(0.015f, thresholds.getValue("speech_rms").jsonPrimitive.float)
        assertEquals(800, thresholds.getValue("silence_ms").jsonPrimitive.int)
        assertEquals(2000, thresholds.getValue("min_speech_ms").jsonPrimitive.int)

        val autoStopIds = fixture.getValue("default_auto_stop_presets").jsonArray
            .map { it.jsonPrimitive.content }
        autoStopIds.forEach { presetId ->
            assertEquals(
                "autoStopRecording for $presetId",
                true,
                requireNotNull(DefaultPresetLookup.byId(presetId)).autoStopRecording,
            )
        }

        val micContract = fixture.getValue("shared_mic_button_contract").jsonObject
        val transcribePreset = requireNotNull(
            DefaultPresetLookup.byId(micContract.getValue("text_input").jsonPrimitive.content),
        )
        assertEquals(
            micContract.getValue("preset_transcribe_prompt").jsonPrimitive.content,
            transcribePreset.blocks.first().prompt,
        )

        fixture.getValue("android_explicit_unsupported_presets").jsonArray.forEach { element ->
            val gap = element.jsonObject
            val resolved = requireNotNull(
                repository.getResolvedPreset(gap.getValue("preset_id").jsonPrimitive.content),
            )
            assertEquals(false, resolved.executionCapability.supported)
            assertEquals(
                gap.getValue("reason").jsonPrimitive.content,
                resolved.executionCapability.reason?.name,
            )
            assertEquals(
                gap.getValue("model_id").jsonPrimitive.content,
                resolved.preset.blocks.first { block ->
                    block.blockType == dev.screengoated.toolbox.mobile.shared.preset.BlockType.AUDIO
                }.model,
            )
        }
    }

    @Test
    fun presetModelCatalogIncludesGemma4FamilyAcrossModalities() {
        val textModel = requireNotNull(PresetModelCatalog.getById("gemma-4-26b-a4b"))
        val visionModel = requireNotNull(PresetModelCatalog.getById("gemma-4-26b-a4b-vision"))

        assertEquals(PresetModelProvider.GOOGLE, textModel.provider)
        assertEquals(PresetModelType.TEXT, textModel.modelType)
        assertEquals(PresetModelType.VISION, visionModel.modelType)
        assertTrue(PresetModelCatalog.forType(PresetModelType.TEXT).any { it.id == "gemma-4-31b" })
        assertTrue(PresetModelCatalog.forType(PresetModelType.VISION).any { it.id == "gemma-4-31b-vision" })
        assertTrue(PresetModelCatalog.getById("gemma-4-26b-a4b-audio") == null)
        assertTrue(PresetModelCatalog.getById("gemma-4-31b-audio") == null)
    }

    @Test
    fun imagePresetWithAudioBlockIsRejectedAsUnsupported() {
        val capability = PresetExecutionCapabilityResolver().resolveExecutionCapability(
            dev.screengoated.toolbox.mobile.shared.preset.Preset(
                id = "image-audio-mixed",
                nameEn = "Image Audio Mixed",
                nameVi = "Image Audio Mixed",
                nameKo = "Image Audio Mixed",
                presetType = dev.screengoated.toolbox.mobile.shared.preset.PresetType.IMAGE,
                blocks = listOf(
                    dev.screengoated.toolbox.mobile.shared.preset.audioBlock("whisper-fast"),
                ),
            ),
        )

        assertFalse(capability.supported)
        assertEquals(PresetPlaceholderReason.NON_TEXT_GRAPH_NOT_READY, capability.reason)
    }

    private fun createRepository(store: PresetOverrideStore): PresetRepository {
        return PresetRepository(
            textApiClient = TextApiClient(OkHttpClient()),
            visionApiClient = VisionApiClient(OkHttpClient()),
            apiKeys = { ApiKeys() },
            runtimeSettings = { PresetRuntimeSettings() },
            uiLanguage = { "en" },
            overrideStore = store,
            toastBus = AppToastBus(),
            mainDispatcher = dispatcher,
        )
    }

    private fun fixtureCases(): List<FixtureCase> {
        val root = json.parseToJsonElement(Files.readAllBytes(fixturePath()).decodeToString()).jsonObject
        return root.getValue("cases").jsonArray.map { case ->
            val jsonObject = case.jsonObject
            FixtureCase(
                name = jsonObject.getValue("name").jsonPrimitive.content,
                presetId = jsonObject.getValue("preset_id").jsonPrimitive.content,
                override = jsonObject["override"]?.jsonObject,
                expectedExecution = jsonObject["expected_execution"]?.jsonObject?.let { execution ->
                    FixtureExecution(
                        supported = execution.getValue("supported").jsonPrimitive.boolean,
                        reason = execution["reason"]?.jsonPrimitive?.contentOrNull,
                    )
                },
            )
        }
    }

    private fun fixturePath(): Path {
        val candidates = listOf(
            Paths.get("..", "parity-fixtures", "preset-system", "catalog-overrides.json"),
            Paths.get("..", "..", "parity-fixtures", "preset-system", "catalog-overrides.json"),
            Paths.get("parity-fixtures", "preset-system", "catalog-overrides.json"),
        )
        return candidates.firstOrNull(Files::exists)
            ?: error("Could not locate preset parity fixture.")
    }

    private fun audioRuntimeFixture(): JsonObject {
        return json.parseToJsonElement(Files.readAllBytes(audioRuntimeFixturePath()).decodeToString()).jsonObject
    }

    private fun audioRuntimeFixturePath(): Path {
        val candidates = listOf(
            Paths.get("..", "parity-fixtures", "preset-system", "audio-runtime.json"),
            Paths.get("..", "..", "parity-fixtures", "preset-system", "audio-runtime.json"),
            Paths.get("parity-fixtures", "preset-system", "audio-runtime.json"),
        )
        return candidates.firstOrNull(Files::exists)
            ?: error("Could not locate audio-runtime parity fixture.")
    }

    private data class FixtureCase(
        val name: String,
        val presetId: String,
        val override: JsonObject?,
        val expectedExecution: FixtureExecution?,
    )

    private data class FixtureExecution(
        val supported: Boolean,
        val reason: String?,
    )

    private class InMemoryPresetOverrideStore(
        private var state: StoredPresetOverrides = StoredPresetOverrides(),
    ) : PresetOverrideStore {
        override fun load(): StoredPresetOverrides = state

        override fun save(overrides: StoredPresetOverrides) {
            state = overrides
        }
    }
}

private object DefaultPresetLookup {
    private val byId = dev.screengoated.toolbox.mobile.shared.preset.DefaultPresets.all.associateBy { it.id }

    fun byId(id: String): dev.screengoated.toolbox.mobile.shared.preset.Preset? = byId[id]
}

private fun JsonObject.toPresetOverride(): PresetOverride {
    return PresetOverride(
        isFavorite = this["is_favorite"]?.jsonPrimitive?.booleanOrNull,
        promptMode = this["prompt_mode"]?.jsonPrimitive?.contentOrNull,
    )
}
