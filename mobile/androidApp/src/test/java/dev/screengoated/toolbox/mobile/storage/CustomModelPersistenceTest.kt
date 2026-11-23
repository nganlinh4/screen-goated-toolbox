package dev.screengoated.toolbox.mobile.storage

import android.content.SharedPreferences
import dev.screengoated.toolbox.mobile.model.CustomModelState
import dev.screengoated.toolbox.mobile.model.CustomModelStatus
import dev.screengoated.toolbox.mobile.preset.CustomPresetModelDefinition
import dev.screengoated.toolbox.mobile.preset.PresetCustomModelRegistry
import java.lang.reflect.Proxy
import java.nio.file.Files
import java.nio.file.Paths
import kotlinx.serialization.json.Json
import kotlinx.serialization.encodeToString
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.contentOrNull
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class CustomModelPersistenceTest {
    @Test fun sharedMalformedDataCasesDisableOnlyCustomModelsWithoutWriting() {
        val fixture = listOf(Paths.get("..", "parity-fixtures"), Paths.get("..", "..", "parity-fixtures"),
            Paths.get("parity-fixtures")).map { it.resolve("preset-system/custom-model-persistence.json") }.first(Files::exists)
        val contract = Json.parseToJsonElement(Files.readAllBytes(fixture).decodeToString()).jsonObject
        assertEquals(1, contract.getValue("schema_version").jsonPrimitive.int)
        for (element in contract.getValue("cases").jsonArray) {
            val case = element.jsonObject
            val payload = case.getValue("payload").jsonPrimitive.contentOrNull
            var writes = 0
            val preferences = Proxy.newProxyInstance(
                SharedPreferences::class.java.classLoader, arrayOf(SharedPreferences::class.java),
            ) { _, method, _ -> when (method.name) {
                "getString" -> payload
                "edit" -> { writes++; error("Unexpected write") }
                else -> error("Unexpected preferences operation: ${method.name}")
            } } as SharedPreferences
            val bootstrap = SecureBootstrap {
                val store = SecureSettingsStore(preferences, Json { ignoreUnknownKeys = true })
                CustomModelState(store::loadCustomModels, { store.saveCustomModels(it) })
            }
            val result = requireNotNull(bootstrap.retry())
            when (case.getValue("expected").jsonPrimitive.content) {
                "loaded" -> {
                    assertTrue(result.available)
                    assertEquals(case.getValue("count").jsonPrimitive.int, result.models.value.size)
                }
                "unavailable" -> {
                    assertEquals("custom_models", case.getValue("scope").jsonPrimitive.content)
                    assertFalse(result.available)
                    assertFalse(result.update(listOf(CustomPresetModelDefinition(id = "x", displayName = "X", fullName = "x"))))
                }
                else -> error("Unknown persistence case")
            }
            assertEquals(0, writes)
            assertEquals(payload, preferences.getString("custom_models", null))
        }
    }

    @Test fun encryptedStoreFailureStillBlocksBootstrap() {
        val preferences = Proxy.newProxyInstance(
            SharedPreferences::class.java.classLoader, arrayOf(SharedPreferences::class.java),
        ) { _, method, _ -> if (method.name == "getString") throw SecurityException("unavailable") else error("Unexpected operation")
        } as SharedPreferences
        val bootstrap = SecureBootstrap {
            val store = SecureSettingsStore(preferences, Json)
            CustomModelState(store::loadCustomModels, { store.saveCustomModels(it) })
        }
        assertNull(bootstrap.retry())
    }

    @Test fun failedSecureCommitRetainsCandidateForExplicitRetry() = runTest {
        val fixture = listOf(Paths.get("..", "parity-fixtures"), Paths.get("..", "..", "parity-fixtures"),
            Paths.get("parity-fixtures")).map { it.resolve("preset-system/custom-model-write-order.json") }.first(Files::exists)
        val case = Json.parseToJsonElement(Files.readAllBytes(fixture).decodeToString()).jsonObject
            .getValue("cases").jsonArray.map { it.jsonObject }
            .single { it.getValue("event").jsonPrimitive.content == "commit_returns_false" }
        val first = CustomPresetModelDefinition(id = "old", displayName = "Old", fullName = "old")
        val second = CustomPresetModelDefinition(id = "new", displayName = "New", fullName = "new")
        var stored = Json.encodeToString(listOf(first))
        var staged = stored
        var failAfterEffect = true
        var attempts = 0
        lateinit var editor: SharedPreferences.Editor
        editor = Proxy.newProxyInstance(
            SharedPreferences.Editor::class.java.classLoader, arrayOf(SharedPreferences.Editor::class.java),
        ) { _, method, args -> when (method.name) {
            "putString" -> { staged = args[1] as String; editor }
            "commit" -> {
                attempts++
                stored = staged
                !failAfterEffect
            }
            else -> error("Unexpected editor operation: ${method.name}")
        } } as SharedPreferences.Editor
        val preferences = Proxy.newProxyInstance(
            SharedPreferences::class.java.classLoader, arrayOf(SharedPreferences::class.java),
        ) { _, method, _ -> when (method.name) {
            "getString" -> stored
            "edit" -> editor
            else -> error("Unexpected preferences operation: ${method.name}")
        } } as SharedPreferences
        val store = SecureSettingsStore(preferences, Json)
        val previous = PresetCustomModelRegistry.definitions()
        try {
            val state = CustomModelState(store::loadCustomModels, { store.saveCustomModels(it) }, StandardTestDispatcher(testScheduler))
            state.publish()
            assertTrue(state.update(listOf(second)))
            advanceUntilIdle()
            assertEquals(CustomModelStatus.WRITE_UNCERTAIN, state.status.value)
            assertEquals(case.getValue("status").jsonPrimitive.content,
                if (state.status.value == CustomModelStatus.WRITE_UNCERTAIN) "uncertain" else "other")
            assertEquals(listOf(second), state.models.value)
            assertEquals(listOf(first), PresetCustomModelRegistry.definitions())
            assertEquals(listOf(second), store.loadCustomModels())
            val laterAccepted = state.update(emptyList())
            assertFalse(laterAccepted)
            assertEquals(case.getValue("later_edits").jsonPrimitive.content, if (laterAccepted) "allowed" else "blocked")
            assertEquals(1, attempts)
            failAfterEffect = false
            assertTrue(state.retry())
            advanceUntilIdle()
            assertEquals(2, attempts)
            assertEquals(listOf(second), state.models.value)
            assertEquals(listOf(second), PresetCustomModelRegistry.definitions())
            assertEquals(listOf(second), store.loadCustomModels())
        } finally { PresetCustomModelRegistry.set(previous) }
    }
}
