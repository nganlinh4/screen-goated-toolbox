package dev.screengoated.toolbox.mobile.preset

import dev.screengoated.toolbox.mobile.shared.preset.BlockType
import dev.screengoated.toolbox.mobile.shared.preset.DefaultPresets
import dev.screengoated.toolbox.mobile.shared.preset.Preset
import dev.screengoated.toolbox.mobile.shared.preset.PresetType
import dev.screengoated.toolbox.mobile.shared.preset.ProcessingBlock
import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import kotlinx.serialization.json.putJsonArray
import kotlinx.serialization.json.putJsonObject
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * Replays the Windows-generated `parity-fixtures/preset-system/default-presets.json`
 * (see `src/config/preset/defaults/parity_tests.rs`) against the Android built-ins.
 */
class DefaultPresetsParityTest {
    private val fixture: List<JsonObject> = Json.parseToJsonElement(
        generateSequence(File(requireNotNull(System.getProperty("user.dir"))).absoluteFile) { it.parentFile }
            .map { File(it, "parity-fixtures/preset-system/default-presets.json") }
            .first(File::isFile)
            .readText(),
    ).jsonObject.getValue("presets").jsonArray.map { it.jsonObject }

    @Test
    fun everyBuiltInMatchesTheWindowsDefaults() {
        val android = DefaultPresets.all.associateBy(Preset::id)
        assertEquals(fixture.map(::id).sorted(), android.keys.sorted())
        val mismatches = fixture.flatMap { expected ->
            val want = flatten(expected, id(expected))
            val have = flatten(project(android.getValue(id(expected))), id(expected))
            (want.keys + have.keys).filter { want[it] != have[it] }
                .map { "$it: expected ${want[it]} but was ${have[it]}" }
        }
        assertEquals(mismatches.joinToString(separator = "\n"), emptyList<String>(), mismatches)
    }

    @Test
    fun eachSectionKeepsTheWindowsDisplayOrder() {
        val android = DefaultPresets.all.map { project(it).jsonObject }
        for (kind in fixture.map(::kind).distinct()) {
            assertEquals(
                kind,
                fixture.filter { kind(it) == kind }.map(::id),
                android.filter { kind(it) == kind }.map(::id),
            )
        }
    }

    /** Leaf values by path, so a failure names each differing field. */
    private fun flatten(value: JsonElement, path: String): Map<String, String> = when (value) {
        is JsonObject -> value.entries.fold(emptyMap()) { leaves, (key, child) -> leaves + flatten(child, "$path.$key") }
        is JsonArray -> value.foldIndexed(mapOf("$path.size" to value.size.toString())) { index, leaves, child ->
            leaves + flatten(child, "$path[$index]")
        }
        else -> mapOf(path to value.toString())
    }

    private fun id(value: JsonObject) = value.getValue("id").jsonPrimitive.content

    private fun kind(value: JsonObject) = value.getValue("kind").jsonPrimitive.content

    private fun project(preset: Preset): JsonElement = buildJsonObject {
        put("id", preset.id)
        put("kind", preset.presetType.wireKind())
        putJsonObject("names") {
            put("en", preset.nameEn)
            put("vi", preset.nameVi)
            put("ko", preset.nameKo)
        }
        put("promptMode", preset.promptMode)
        put("autoPaste", preset.autoPaste)
        put("autoPasteNewline", preset.autoPasteNewline)
        put("hideRecordingUi", preset.hideRecordingUi)
        put("autoStopRecording", preset.autoStopRecording)
        put("continuousInput", preset.continuousInput)
        put("isMaster", preset.isMaster)
        put("isFavorite", preset.isFavorite)
        put("isUpcoming", preset.isUpcoming)
        put("showControllerUi", preset.showControllerUi)
        put("graphExplicit", preset.graphExplicit)
        putJsonArray("blockConnections") {
            preset.blockConnections.forEach { (from, to) ->
                add(buildJsonArray { add(JsonPrimitive(from)); add(JsonPrimitive(to)) })
            }
        }
        putJsonArray("blocks") { preset.blocks.forEach { add(project(it)) } }
    }

    private fun project(block: ProcessingBlock): JsonElement = buildJsonObject {
        put("type", block.blockType.wireName())
        if (block.blockType == BlockType.INPUT_ADAPTER) {
            // Pass-through: only output behavior is meaningful, as on Windows.
            put("renderMode", block.renderMode)
            put("showOverlay", block.showOverlay)
            put("autoCopy", block.autoCopy)
            put("autoSpeak", block.autoSpeak)
            return@buildJsonObject
        }
        put("model", block.model)
        put("prompt", block.prompt)
        putJsonObject("languageVars") { block.languageVars.toSortedMap().forEach { (key, value) -> put(key, value) } }
        put("streamingEnabled", block.streamingEnabled)
        put("searchEnabled", block.searchEnabled)
        put("renderMode", block.renderMode)
        put("showOverlay", block.showOverlay)
        put("autoCopy", block.autoCopy)
        put("autoSpeak", block.autoSpeak)
    }

    private fun PresetType.wireKind() = name.lowercase()

    private fun BlockType.wireName() = name.lowercase()
}
