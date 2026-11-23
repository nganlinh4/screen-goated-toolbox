package dev.screengoated.toolbox.mobile

import dev.screengoated.toolbox.mobile.phonecontrol.memory.PhoneControlMemoryRepository
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlMemoryStartup
import dev.screengoated.toolbox.mobile.preset.CustomPresetModelDefinition
import dev.screengoated.toolbox.mobile.preset.PresetCustomModelRegistry
import java.nio.file.Files
import java.nio.file.Paths
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class AppStartupOrderTest {
    @get:Rule val temporaryFolder = TemporaryFolder()

    @Test fun failedMemoryRecoveryDoesNotPublishLaterOwnersAndRetryCanProceed() {
        val fixture = listOf(Paths.get("..", "parity-fixtures"), Paths.get("..", "..", "parity-fixtures"),
            Paths.get("parity-fixtures")).map { it.resolve("phone-control/app-startup-order.json") }.first(Files::exists)
        val contract = Json.parseToJsonElement(Files.readAllBytes(fixture).decodeToString()).jsonObject
        assertEquals(1, contract.getValue("schema_version").jsonPrimitive.int)
        val cases = contract.getValue("cases").jsonArray.associate { element ->
            val case = element.jsonObject
            case.getValue("event").jsonPrimitive.content to case
        }
        assertEquals(setOf("recovery_failure", "recovery_success"), cases.keys)
        val previous = PresetCustomModelRegistry.definitions()
        val old = listOf(CustomPresetModelDefinition(id = "old", displayName = "Old", fullName = "old"))
        val fresh = listOf(CustomPresetModelDefinition(id = "new", displayName = "New", fullName = "new"))
        val repository = PhoneControlMemoryRepository(temporaryFolder.newFolder())
        var clock = -1L
        val startup = PhoneControlMemoryStartup(repository) { clock }
        var observerStarts = 0
        fun attempt() {
            prepareAppStartup(recover = { startup.recoverOnce() }, publishModels = { PresetCustomModelRegistry.set(fresh) })
            observerStarts++
        }
        try {
            PresetCustomModelRegistry.set(old)
            assertThrows(IllegalArgumentException::class.java) { attempt() }
            val failed = cases.getValue("recovery_failure")
            assertEquals(failed.getValue("custom_models").jsonPrimitive.content,
                if (PresetCustomModelRegistry.definitions() == old) "unchanged" else "published")
            assertEquals(failed.getValue("preset_observer").jsonPrimitive.content,
                if (observerStarts == 0) "not_started" else "may_start")
            clock = 100L
            attempt()
            val succeeded = cases.getValue("recovery_success")
            assertEquals(succeeded.getValue("custom_models").jsonPrimitive.content,
                if (PresetCustomModelRegistry.definitions() == fresh) "published" else "unchanged")
            assertEquals(succeeded.getValue("preset_observer").jsonPrimitive.content,
                if (observerStarts == 1) "may_start" else "not_started")
        } finally {
            PresetCustomModelRegistry.set(previous)
        }
    }
}
