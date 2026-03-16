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

    private fun assetPath(): Path {
        val candidates = listOf(
            Paths.get("src", "main", "assets", "preset_overlay", "button_canvas.js"),
            Paths.get("mobile", "androidApp", "src", "main", "assets", "preset_overlay", "button_canvas.js"),
        )
        return candidates.firstOrNull(Files::exists)
            ?: error("Could not locate preset button canvas asset.")
    }
}
