package dev.screengoated.toolbox.mobile.service.preset

import dev.screengoated.toolbox.mobile.preset.ResolvedPreset
import dev.screengoated.toolbox.mobile.preset.triLang
import dev.screengoated.toolbox.mobile.service.overlay.overlayFontCss
import dev.screengoated.toolbox.mobile.shared.preset.PresetType
import dev.screengoated.toolbox.mobile.ui.icons.MaterialSymbolSvgs

internal enum class FavoriteBubbleSide(val wireValue: String) {
    LEFT("left"),
    RIGHT("right"),
}

internal data class FavoriteBubblePanelSettings(
    val favorites: List<ResolvedPreset>,
    val lang: String,
    val isDark: Boolean,
    val keepOpenEnabled: Boolean,
    val columnCount: Int,
)

internal class FavoriteBubbleHtmlBuilder(
    private val symbols: MaterialSymbolSvgs,
) {
    fun build(settings: FavoriteBubblePanelSettings): String {
        val replacements = linkedMapOf(
            "FONT_CSS" to overlayFontCss(),
            "PANEL_CSS" to favoriteBubblePanelCss(settings.isDark),
            "KEEP_OPEN_LABEL" to htmlEscape(
                localize(settings.lang, "Keep open", "Giữ mở", "계속 열기"),
            ),
            "KEEP_OPEN_CLASS" to if (settings.keepOpenEnabled) " active" else "",
            "KEEP_OPEN_DEFAULT" to if (settings.keepOpenEnabled) "true" else "false",
            "COLUMN_COUNT" to settings.columnCount.toString(),
            "FAVORITES_HTML" to favoritesHtml(settings),
            "PANEL_JS" to favoriteBubblePanelJavascript(),
        )
        return replacements.entries.fold(favoriteBubbleBaseHtmlTemplate()) { html, (token, value) ->
            html.replace("{{$token}}", value)
        }
    }

    private fun favoritesHtml(settings: FavoriteBubblePanelSettings): String {
        val favorites = settings.favorites.filter { !it.preset.isUpcoming }
        if (favorites.isEmpty()) {
            return """<div class="empty">${htmlEscape(emptyFavoritesMessage(settings.lang))}</div>"""
        }

        return favorites.mapIndexed { index, preset ->
            val (iconSvg, colorHex) = presetIcon(preset, settings.isDark)
            val label = htmlEscape(preset.preset.name(settings.lang))
            """
            <div class="preset-item" data-index="$index" onmousedown="onMouseDown($index)" onmouseup="onMouseUp($index)" onmouseleave="onMouseLeave()">
                <div class="progress-fill"></div>
                <span class="icon" style="color: $colorHex;">$iconSvg</span>
                <span class="name">$label</span>
            </div>
            """.trimIndent()
        }.joinToString("\n")
    }

    private fun presetIcon(
        preset: ResolvedPreset,
        isDark: Boolean,
    ): Pair<String, String> {
        val (symbol, colorHex) = when (preset.preset.presetType) {
            PresetType.IMAGE -> "image" to if (isDark) "#44ccff" else "#1976d2"
            PresetType.TEXT_SELECT -> "format_italic" to if (isDark) "#55ff88" else "#388e3c"
            PresetType.TEXT_INPUT -> "keyboard" to if (isDark) "#55ff88" else "#388e3c"
            PresetType.MIC -> "mic" to if (isDark) "#ffaa33" else "#f57c00"
            PresetType.DEVICE_AUDIO -> "speaker_group" to if (isDark) "#ffaa33" else "#f57c00"
        }
        return symbols.svg(symbol, sizePx = ICON_SIZE_PX) to colorHex
    }

    private fun htmlEscape(text: String): String {
        return text
            .replace("&", "&amp;")
            .replace("<", "&lt;")
            .replace(">", "&gt;")
            .replace("\"", "&quot;")
    }
}

private fun localize(
    lang: String,
    en: String,
    vi: String,
    ko: String,
): String = triLang(lang, en, vi, ko)

private const val ICON_SIZE_PX = 20
