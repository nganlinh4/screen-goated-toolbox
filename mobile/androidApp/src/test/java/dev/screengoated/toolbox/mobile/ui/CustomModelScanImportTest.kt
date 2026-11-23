package dev.screengoated.toolbox.mobile.ui

import dev.screengoated.toolbox.mobile.preset.PresetModelProvider
import java.nio.file.Files
import java.nio.file.Paths
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Test

class CustomModelScanImportTest {
    @Test fun scanImportsUseDistinctNamesAndIds() {
        val path = listOf(Paths.get("..", "parity-fixtures"), Paths.get("..", "..", "parity-fixtures"),
            Paths.get("parity-fixtures")).map { it.resolve("preset-system/custom-model-scan-imports.json") }.first(Files::exists)
        val cases = Json.parseToJsonElement(Files.readAllBytes(path).decodeToString()).jsonObject.getValue("cases").jsonArray
        for (entry in cases) {
            val case = entry.jsonObject
            val name = case.getValue("name").jsonPrimitive.content
            fun strings(key: String) = case.getValue(key).jsonArray.map { it.jsonPrimitive.content }
            val existing = strings("existing").map { newCustomModel(PresetModelProvider.OLLAMA, emptyList(), it) }
            val scanned = strings("scanned").map { newCustomModel(PresetModelProvider.OLLAMA, emptyList(), it) }
            val additions = planOllamaScan(scanned, existing)
            assertEquals(name, strings("added"), additions.map { it.fullName })
            assertEquals(name, strings("ids"), additions.map { it.id })
        }
    }
}
