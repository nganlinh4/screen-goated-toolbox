package dev.screengoated.toolbox.mobile.service.preset

import dev.screengoated.toolbox.mobile.preset.ResolvedPreset
import dev.screengoated.toolbox.mobile.preset.triLang
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
): String = triLang(lang, en, vi, ko)

private const val WINDOWS_IMAGE_ICON =
    """<svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor"><path d="M5 21q-.825 0-1.412-.587T3 19V5q0-.825.588-1.412T5 3h14q.825 0 1.413.588T21 5v14q0 .825-.587 1.413T19 21zm1-4h12l-3.75-5l-3 4L9 13z"/></svg>"""
private const val WINDOWS_TEXT_TYPE_ICON =
    """<svg width="20" height="20" viewBox="0 -960 960 960" fill="currentColor"><path d="M160-200q-33 0-56.5-23.5T80-280v-400q0-33 23.5-56.5T160-760h640q33 0 56.5 23.5T880-680v400q0 33-23.5 56.5T800-200H160Zm200-120h240q17 0 28.5-11.5T640-360q0-17-11.5-28.5T600-400H360q-17 0-28.5 11.5T320-360q0 17 11.5 28.5T360-320ZM240-560q17 0 28.5-11.5T280-600q0-17-11.5-28.5T240-640q-17 0-28.5 11.5T200-600q0 17 11.5 28.5T240-560Zm120 0q17 0 28.5-11.5T400-600q0-17-11.5-28.5T360-640q-17 0-28.5 11.5T320-600q0 17 11.5 28.5T360-560Zm120 0q17 0 28.5-11.5T520-600q0-17-11.5-28.5T480-640q-17 0-28.5 11.5T440-600q0 17 11.5 28.5T480-560Zm120 0q17 0 28.5-11.5T640-600q0-17-11.5-28.5T600-640q-17 0-28.5 11.5T560-600q0 17 11.5 28.5T600-560Zm120 0q17 0 28.5-11.5T760-600q0-17-11.5-28.5T720-640q-17 0-28.5 11.5T680-600q0 17 11.5 28.5T720-560ZM240-440q17 0 28.5-11.5T280-480q0-17-11.5-28.5T240-520q-17 0-28.5 11.5T200-480q0 17 11.5 28.5T240-440Zm120 0q17 0 28.5-11.5T400-480q0-17-11.5-28.5T360-520q-17 0-28.5 11.5T320-480q0 17 11.5 28.5T360-440Zm120 0q17 0 28.5-11.5T520-480q0-17-11.5-28.5T480-520q-17 0-28.5 11.5T440-480q0 17 11.5 28.5T480-440Zm120 0q17 0 28.5-11.5T640-480q0-17-11.5-28.5T600-520q-17 0-28.5 11.5T560-480q0 17 11.5 28.5T600-440Zm120 0q17 0 28.5-11.5T760-480q0-17-11.5-28.5T720-520q-17 0-28.5 11.5T680-480q0 17 11.5 28.5T720-440Z"/></svg>"""
private const val WINDOWS_TEXT_SELECT_ICON =
    """<svg width="20" height="20" viewBox="0 -960 960 960" fill="currentColor"><path d="M250-200q-21 0-35.5-14.5T200-250q0-21 14.5-35.5T250-300h110l120-360H370q-21 0-35.5-14.5T320-710q0-21 14.5-35.5T370-760h300q21 0 35.5 14.5T720-710q0 21-14.5 35.5T670-660h-90L460-300h90q21 0 35.5 14.5T600-250q0 21-14.5 35.5T550-200H250Z"/></svg>"""
private const val WINDOWS_MIC_ICON =
    """<svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor"><path d="M9.875 13.125Q9 12.25 9 11V5q0-1.25.875-2.125T12 2t2.125.875T15 5v6q0 1.25-.875 2.125T12 14t-2.125-.875M11 21v-3.075q-2.6-.35-4.3-2.325T5 11h2q0 2.075 1.463 3.538T12 16t3.538-1.463T17 11h2q0 2.625-1.7 4.6T13 17.925V21z"/></svg>"""
private const val WINDOWS_DEVICE_ICON =
    """<svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor"><path d="M10 19q-.825 0-1.412-.587T8 17V3q0-.825.588-1.412T10 1h9q.825 0 1.413.588T21 3v14q0 .825-.587 1.413T19 19zm4.5-11.5q.625 0 1.063-.437T16 6t-.437-1.062T14.5 4.5t-1.062.438T13 6t.438 1.063T14.5 7.5m0 8.5q1.45 0 2.475-1.025T18 12.5t-1.025-2.475T14.5 9t-2.475 1.025T11 12.5t1.025 2.475T14.5 16m0-2q-.625 0-1.062-.437T13 12.5t.438-1.062T14.5 11t1.063.438T16 12.5t-.437 1.063T14.5 14m1.5 9H6q-.825 0-1.412-.587T4 21V5h2v16h10z"/></svg>"""
private const val WINDOWS_REALTIME_ICON =
    """<svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor"><path d="M7 18V6h2v12zm4 4V2h2v20zm-8-8v-4h2v4zm12 4V6h2v12zm4-4v-4h2v4z"/></svg>"""
