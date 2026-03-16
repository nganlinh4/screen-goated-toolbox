package dev.screengoated.toolbox.mobile.service.preset

import dev.screengoated.toolbox.mobile.preset.PresetExecutionCapability
import dev.screengoated.toolbox.mobile.preset.ResolvedPreset
import dev.screengoated.toolbox.mobile.shared.preset.DefaultPresets
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.Paths
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PresetOverlayHtmlTest {
    @Test
    fun buttonCanvasAssetStaysMarkdownOnly() {
        val asset = Files.readAllBytes(assetPath()).decodeToString()

        assertTrue(asset.contains("copyButton"))
        assertTrue(asset.contains("closeButton"))
        assertFalse(asset.contains("plainTextLabel"))
        assertFalse(asset.contains("markdownLabel"))
    }

    @Test
    fun emptyFavoritesMessageIsPresentInPanelBootstrap() {
        val payload = emptyFavoritesMessage(lang = "en")

        assertTrue(payload.contains("No favorite presets yet. Star presets in the app first."))
    }

    @Test
    fun favoriteBubbleBuilderKeepsWindowsPanelHooks() {
        val preset = DefaultPresets.all.first { it.id == "preset_ask_ai" }
        val html = FavoriteBubbleHtmlBuilder().build(
            FavoriteBubblePanelSettings(
                favorites = listOf(
                    ResolvedPreset(
                        preset = preset.copy(isFavorite = true),
                        hasOverride = false,
                        isBuiltIn = true,
                        executionCapability = PresetExecutionCapability(supported = true),
                        placeholderReasons = emptySet(),
                    ),
                ),
                lang = "en",
                isDark = true,
                keepOpenEnabled = false,
                columnCount = 1,
            ),
        )

        assertTrue(html.contains("keep-open-row visible"))
        assertTrue(html.contains("toggleKeepOpen()"))
        assertTrue(html.contains("resizeBubble('desc')"))
        assertTrue(html.contains("resizeBubble('inc')"))
        assertTrue(html.contains("function animateIn"))
        assertTrue(html.contains("function closePanel"))
        assertTrue(html.contains("function showItemsImmediately"))
        assertTrue(html.contains("focus_bubble"))
        assertTrue(html.contains("close_now"))
        assertFalse(html.contains("panelTitle"))
    }

    @Test
    fun favoriteBubbleBuilderMarksKeepOpenAsActiveWhenEnabled() {
        val preset = DefaultPresets.all.first { it.id == "preset_ask_ai" }
        val html = FavoriteBubbleHtmlBuilder().build(
            FavoriteBubblePanelSettings(
                favorites = listOf(
                    ResolvedPreset(
                        preset = preset.copy(isFavorite = true),
                        hasOverride = false,
                        isBuiltIn = true,
                        executionCapability = PresetExecutionCapability(supported = true),
                        placeholderReasons = emptySet(),
                    ),
                ),
                lang = "en",
                isDark = true,
                keepOpenEnabled = true,
                columnCount = 1,
            ),
        )

        assertTrue(html.contains("keep-open-label active"))
        assertTrue(html.contains("let keepOpen = true;"))
    }

    @Test
    fun favoriteBubbleBuilderSetsRequestedColumnCount() {
        val preset = DefaultPresets.all.first { it.id == "preset_ask_ai" }
        val html = FavoriteBubbleHtmlBuilder().build(
            FavoriteBubblePanelSettings(
                favorites = List(2) { index ->
                    ResolvedPreset(
                        preset = preset.copy(id = "${preset.id}_$index", isFavorite = true),
                        hasOverride = false,
                        isBuiltIn = true,
                        executionCapability = PresetExecutionCapability(supported = true),
                        placeholderReasons = emptySet(),
                    )
                },
                lang = "en",
                isDark = true,
                keepOpenEnabled = false,
                columnCount = 2,
            ),
        )

        assertTrue(html.contains("""column-count: 2;"""))
    }

    @Test
    fun textInputBuilderKeepsWindowsOverlayHooks() {
        val html = PresetTextInputHtmlBuilder().build(
            PresetTextInputHtmlSettings(
                lang = "en",
                title = "Ask AI",
                placeholder = "Ready...",
                isDark = true,
            ),
        )

        assertTrue(html.contains("""<div class="editor-container">"""))
        assertTrue(html.contains("""<div class="header" id="headerRegion">"""))
        assertTrue(html.contains("""<textarea id="editor" placeholder="Ready..." autofocus></textarea>"""))
        assertTrue(html.contains("window.ipc.postMessage('drag_window')"))
        assertTrue(html.contains("window.ipc.postMessage('close_window')"))
        assertTrue(html.contains("window.ipc.postMessage('submit:' + text)"))
        assertTrue(html.contains("window.ipc.postMessage('history_up:' + editor.value)"))
        assertTrue(html.contains("window.ipc.postMessage('history_down:' + editor.value)"))
        assertTrue(html.contains("window.ipc.postMessage('mic')"))
        assertTrue(html.contains("window.setEditorText = (text) =>"))
        assertTrue(html.contains("window.updateTheme = (isDark) =>"))
        assertTrue(html.contains("window.playEntry = () =>"))
        assertTrue(html.contains("window.playExit = () =>"))
        assertTrue(html.contains("window.clearInput = () =>"))
        assertTrue(html.contains("""type: 'dragInputWindow'"""))
        assertTrue(html.contains("const TOUCH_DRAG_GAIN = Math.max(window.devicePixelRatio || 1, 1.5);"))
        assertTrue(html.contains("window.ipc.postMessage('dragAt:' + Math.round(touch.screenX) + ',' + Math.round(touch.screenY))"))
        assertTrue(html.contains("window.ipc.postMessage('dragEnd:' + Math.round(point.screenX) + ',' + Math.round(point.screenY))"))
        assertFalse(html.contains("applyInputBootstrap"))
        assertFalse(html.contains("""id="footerRegion""""))
    }

    private fun assetPath(): Path {
        val candidates = listOf(
            Paths.get("src", "main", "assets", "preset_overlay", "button_canvas.js"),
            Paths.get("mobile", "androidApp", "src", "main", "assets", "preset_overlay", "button_canvas.js"),
        )
        return candidates.firstOrNull(Files::exists)
            ?: error("Could not locate preset button canvas asset.")
    }
}
