package dev.screengoated.toolbox.mobile.preset

import dev.screengoated.toolbox.mobile.preset.ui.nodeGraphAddLanguageLabel
import dev.screengoated.toolbox.mobile.preset.ui.nodeGraphLanguageSearchPlaceholder
import dev.screengoated.toolbox.mobile.preset.ui.nodeGraphModelLabel
import dev.screengoated.toolbox.mobile.preset.ui.nodeGraphPromptLabel
import dev.screengoated.toolbox.mobile.preset.ui.nodeGraphPromptPlaceholder
import dev.screengoated.toolbox.mobile.preset.ui.nodeGraphStreamLabel
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.Paths
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Test

class PresetNodeGraphEditorLocaleTest {
    private val json = Json { ignoreUnknownKeys = true }

    @Test
    fun inlineNodeEditorChromeComesFromLanguage() {
        val fixture = json.parseToJsonElement(
            Files.readAllBytes(fixturePath()).decodeToString(),
        ).jsonObject
        val case = fixture.getValue("cases").jsonArray
            .first { it.jsonObject.getValue("name").jsonPrimitive.content == "inline_node_editor_chrome_comes_from_language" }
            .jsonObject

        case.getValue("locales").jsonArray.forEach { entry ->
            val locale = entry.jsonObject
            val lang = locale.getValue("lang").jsonPrimitive.content

            assertEquals(locale.getValue("expected_model_label").jsonPrimitive.content, nodeGraphModelLabel(lang))
            assertEquals(locale.getValue("expected_prompt_label").jsonPrimitive.content, nodeGraphPromptLabel(lang))
            assertEquals(locale.getValue("expected_add_language").jsonPrimitive.content, nodeGraphAddLanguageLabel(lang))
            assertEquals(locale.getValue("expected_prompt_placeholder").jsonPrimitive.content, nodeGraphPromptPlaceholder(lang))
            assertEquals(locale.getValue("expected_language_search").jsonPrimitive.content, nodeGraphLanguageSearchPlaceholder(lang))
            assertEquals(locale.getValue("expected_stream_on").jsonPrimitive.content, nodeGraphStreamLabel(lang, true))
            assertEquals(locale.getValue("expected_stream_off").jsonPrimitive.content, nodeGraphStreamLabel(lang, false))
        }
    }

    private fun fixturePath(): Path {
        val candidates = listOf(
            Paths.get("..", "parity-fixtures", "preset-system", "node-graph-editor.json"),
            Paths.get("..", "..", "parity-fixtures", "preset-system", "node-graph-editor.json"),
            Paths.get("parity-fixtures", "preset-system", "node-graph-editor.json"),
        )
        return candidates.firstOrNull(Files::exists)
            ?: error("Could not locate node graph editor parity fixture.")
    }
}
