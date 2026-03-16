package dev.screengoated.toolbox.mobile.preset

import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.StandardTestDispatcher
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

    private fun createRepository(store: PresetOverrideStore): PresetRepository {
        return PresetRepository(
            textApiClient = TextApiClient(OkHttpClient()),
            apiKeys = { ApiKeys() },
            overrideStore = store,
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

private fun JsonObject.toPresetOverride(): PresetOverride {
    return PresetOverride(
        isFavorite = this["is_favorite"]?.jsonPrimitive?.booleanOrNull,
        promptMode = this["prompt_mode"]?.jsonPrimitive?.contentOrNull,
    )
}
