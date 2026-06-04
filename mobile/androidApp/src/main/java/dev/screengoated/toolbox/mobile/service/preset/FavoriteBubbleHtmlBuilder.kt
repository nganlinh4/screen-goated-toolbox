package dev.screengoated.toolbox.mobile.service.preset

import dev.screengoated.toolbox.mobile.preset.ResolvedPreset
import dev.screengoated.toolbox.mobile.service.overlay.overlayFontCss
import dev.screengoated.toolbox.mobile.shared.preset.PresetType

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

internal class FavoriteBubbleHtmlBuilder {
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
        return when (preset.preset.presetType) {
            PresetType.IMAGE -> WINDOWS_IMAGE_ICON to if (isDark) "#44ccff" else "#1976d2"
            PresetType.TEXT_SELECT -> WINDOWS_TEXT_SELECT_ICON to if (isDark) "#55ff88" else "#388e3c"
            PresetType.TEXT_INPUT -> WINDOWS_TEXT_TYPE_ICON to if (isDark) "#55ff88" else "#388e3c"
            PresetType.MIC -> {
                if (preset.preset.audioProcessingMode == "realtime") {
                    WINDOWS_REALTIME_ICON to if (isDark) "#ff5555" else "#d32f2f"
                } else {
                    WINDOWS_MIC_ICON to if (isDark) "#ffaa33" else "#f57c00"
                }
            }
            PresetType.DEVICE_AUDIO -> WINDOWS_DEVICE_ICON to if (isDark) "#ffaa33" else "#f57c00"
        }
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
): String = when (lang) {
    "vi" -> vi
    "ko" -> ko
    else -> en
}

private const val WINDOWS_IMAGE_ICON =
    """<svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor"><path d="M12 8.8a3.2 3.2 0 1 0 0 6.4 3.2 3.2 0 0 0 0-6.4z"/><path d="M9 2L7.17 4H4c-1.1 0-2 .9-2 2v12c0 1.1.9 2 2 2h16c1.1 0 2-.9 2-2V6c0-1.1-.9-2-2-2h-3.17L15 2H9zm3 15c-2.76 0-5-2.24-5-5s2.24-5 5-5 5 2.24 5 5-2.24 5-5 5z"/></svg>"""
private const val WINDOWS_TEXT_TYPE_ICON =
    """<svg width="20" height="20" viewBox="0 -960 960 960" fill="currentColor"><path d="M160-200q-33 0-56.5-23.5T80-280v-400q0-33 23.5-56.5T160-760h640q33 0 56.5 23.5T880-680v400q0 33-23.5 56.5T800-200H160Zm200-120h240q17 0 28.5-11.5T640-360q0-17-11.5-28.5T600-400H360q-17 0-28.5 11.5T320-360q0 17 11.5 28.5T360-320ZM240-560q17 0 28.5-11.5T280-600q0-17-11.5-28.5T240-640q-17 0-28.5 11.5T200-600q0 17 11.5 28.5T240-560Zm120 0q17 0 28.5-11.5T400-600q0-17-11.5-28.5T360-640q-17 0-28.5 11.5T320-600q0 17 11.5 28.5T360-560Zm120 0q17 0 28.5-11.5T520-600q0-17-11.5-28.5T480-640q-17 0-28.5 11.5T440-600q0 17 11.5 28.5T480-560Zm120 0q17 0 28.5-11.5T640-600q0-17-11.5-28.5T600-640q-17 0-28.5 11.5T560-600q0 17 11.5 28.5T600-560Zm120 0q17 0 28.5-11.5T760-600q0-17-11.5-28.5T720-640q-17 0-28.5 11.5T680-600q0 17 11.5 28.5T720-560ZM240-440q17 0 28.5-11.5T280-480q0-17-11.5-28.5T240-520q-17 0-28.5 11.5T200-480q0 17 11.5 28.5T240-440Zm120 0q17 0 28.5-11.5T400-480q0-17-11.5-28.5T360-520q-17 0-28.5 11.5T320-480q0 17 11.5 28.5T360-440Zm120 0q17 0 28.5-11.5T520-480q0-17-11.5-28.5T480-520q-17 0-28.5 11.5T440-480q0 17 11.5 28.5T480-440Zm120 0q17 0 28.5-11.5T640-480q0-17-11.5-28.5T600-520q-17 0-28.5 11.5T560-480q0 17 11.5 28.5T600-440Zm120 0q17 0 28.5-11.5T760-480q0-17-11.5-28.5T720-520q-17 0-28.5 11.5T680-480q0 17 11.5 28.5T720-440Z"/></svg>"""
private const val WINDOWS_TEXT_SELECT_ICON =
    """<svg width="20" height="20" viewBox="0 -960 960 960" fill="currentColor"><path d="M250-200q-21 0-35.5-14.5T200-250q0-21 14.5-35.5T250-300h110l120-360H370q-21 0-35.5-14.5T320-710q0-21 14.5-35.5T370-760h300q21 0 35.5 14.5T720-710q0 21-14.5 35.5T670-660h-90L460-300h90q21 0 35.5 14.5T600-250q0 21-14.5 35.5T550-200H250Z"/></svg>"""
private const val WINDOWS_MIC_ICON =
    """<svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor"><path d="M12 14c1.66 0 3-1.34 3-3V5c0-1.66-1.34-3-3-3S9 3.34 9 5v6c0 1.66 1.34 3 3 3zM17 11c0 2.76-2.24 5-5 5s-5-2.24-5-5H5c0 3.53 2.61 6.43 6 6.92V21h2v-3.08c3.39-.49 6-3.39 6-6.92h-2z"/></svg>"""
private const val WINDOWS_DEVICE_ICON =
    """<svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor"><path d="M3 9v6h4l5 5V4L7 9H3zm13.5 3c0-1.77-1.02-3.29-2.5-4.03v8.05c1.48-.73 2.5-2.25 2.5-4.02zM14 3.23v2.06c2.89.86 5 3.54 5 6.71s-2.11 5.85-5 6.71v2.06c4.01-.91 7-4.49 7-8.77s-2.99-7.86-7-8.77z"/></svg>"""
private const val WINDOWS_REALTIME_ICON =
    """<svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M2 12h3 l1.5-3 l2 10 l3.5-14 l3.5 10 l2-3 h4.5"/></svg>"""
