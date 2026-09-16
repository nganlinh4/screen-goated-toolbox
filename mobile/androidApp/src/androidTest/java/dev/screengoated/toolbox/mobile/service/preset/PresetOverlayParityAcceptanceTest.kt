package dev.screengoated.toolbox.mobile.service.preset

import androidx.test.ext.junit.runners.AndroidJUnit4
import dev.screengoated.toolbox.mobile.model.MobileThemeMode
import dev.screengoated.toolbox.mobile.preset.PresetExecutionState
import dev.screengoated.toolbox.mobile.preset.PresetResultWindowId
import dev.screengoated.toolbox.mobile.preset.PresetResultWindowState
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class PresetOverlayParityAcceptanceTest {
    @Test fun continuousInputSubmitsTwiceWithoutPersistingOverride() = PresetUiAcceptanceSupport().use { ui ->
        val id = ui.preset()
        ui.main { ui.controller.launchPreset(id, closePanel = true, continuousMode = true) }
        val window = ui.main { requireNotNull(ui.controller.inputModule.currentWindow()) }
        ui.await("input editor") { ui.js(window, "typeof window.setEditorText === 'function'") == true }
        for (text in listOf("First input", "Second input")) {
            ui.js(window, "window.setEditorText(${JSONObject.quote(text)}); window.focusEditor(); true")
            ui.tap(window, "#sendBtn")
            ui.await("submitted $text") {
                ui.repository.executionState.value.resultWindows.any { it.markdownText == text }
            }
            ui.await("cleared focused editor") {
                ui.js(window, "document.activeElement.id === 'editor' && document.activeElement.value === ''") == true
            }
            assertTrue(ui.main { ui.controller.inputModule.hasWindow() })
            assertFalse(requireNotNull(ui.repository.getResolvedPreset(id)).preset.continuousInput)
        }
        ui.main { ui.controller.launchPreset(id, closePanel = true, continuousMode = false) }
        val single = ui.main { requireNotNull(ui.controller.inputModule.currentWindow()) }
        ui.await("single input editor") { ui.js(single, "typeof window.setEditorText === 'function'") == true }
        ui.js(single, "window.setEditorText('Single input'); window.focusEditor(); true")
        ui.tap(single, "#sendBtn")
        ui.await("single input closes") { !ui.main { ui.controller.inputModule.hasWindow() } }
    }

    @Test fun markdownTogglePreservesLiteralTextAcrossStreamingAndTheme() = PresetUiAcceptanceSupport().use { ui ->
        val id = ui.preset()
        val resolved = requireNotNull(ui.repository.getResolvedPreset(id))
        val resultId = PresetResultWindowId("parity-result", 0)
        var source = "**Bold**\n<literal> & text"
        fun render(raw: Boolean = false) = ui.main {
            ui.controller.resultModule.renderExecutionState(PresetExecutionState(
                activePresetId = id,
                resultWindows = listOf(PresetResultWindowState(resultId, 0, "Parity result",
                    markdownText = source, renderMode = if (raw) "html" else "markdown_stream")),
            ), resolved)
        }
        render()
        val module = ui.controller.resultModule
        ui.await("result controls") { ui.main { module.canvasWindow != null && module.resultWindows[resultId] != null } }
        val result = ui.main { requireNotNull(module.resultWindows[resultId]).window }
        ui.tap(result, "body")
        ui.tap(requireNotNull(ui.main { module.canvasWindow }), "[data-action=markdown]")
        ui.await("plain mode") { ui.main { module.resultWindows[resultId]?.runtimeState?.isMarkdown == false } }
        source += "\n<script>window.unwanted=true</script>"
        render()
        ui.main { ui.preferences.value = ui.preferences.value.copy(themeMode = MobileThemeMode.DARK) }
        ui.await("literal source preserved") {
            ui.js(result, "document.body.innerText.includes(${JSONObject.quote(source)}) && window.unwanted !== true") == true
        }
        assertFalse(ui.main { requireNotNull(module.resultWindows[resultId]).runtimeState.isMarkdown })
        ui.tap(result, "body")
        ui.tap(requireNotNull(ui.main { module.canvasWindow }), "[data-action=markdown]")
        ui.await("markdown restored") { ui.main { module.resultWindows[resultId]?.runtimeState?.isMarkdown == true } }
        source = "<html><body><button id='authored'>Authored control</button></body></html>"
        render(raw = true)
        ui.await("raw HTML hides toggle") {
            ui.js(requireNotNull(ui.main { module.canvasWindow }), """(() => {
                const toggle=document.querySelector('[data-action=markdown]');
                return toggle?.hidden === true && toggle.getBoundingClientRect().width === 0;
            })()""") == true
        }
    }
}
