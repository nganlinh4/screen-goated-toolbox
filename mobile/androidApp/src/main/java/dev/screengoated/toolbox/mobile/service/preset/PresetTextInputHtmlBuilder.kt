package dev.screengoated.toolbox.mobile.service.preset

import dev.screengoated.toolbox.mobile.service.overlay.overlayFontCss

internal data class PresetTextInputHtmlSettings(
    val lang: String,
    val title: String,
    val placeholder: String,
    val isDark: Boolean,
)

internal class PresetTextInputHtmlBuilder {
    fun build(settings: PresetTextInputHtmlSettings): String {
        val replacements = linkedMapOf(
            "THEME_ATTR" to if (settings.isDark) {
                "data-theme=\"dark\""
            } else {
                "data-theme=\"light\""
            },
            "FONT_CSS" to overlayFontCss(),
            "EDITOR_CSS" to presetTextInputCss(settings.isDark),
            "TITLE_TEXT" to htmlEscape(settings.title),
            "PLACEHOLDER_TEXT" to htmlEscape(settings.placeholder),
            "CLOSE_SVG" to WINDOWS_CLOSE_ICON,
            "MIC_SVG" to WINDOWS_MIC_ICON,
            "SEND_SVG" to WINDOWS_SEND_ICON,
            "EDITOR_JS" to presetTextInputJavascript(),
        )
        return replacements.entries.fold(presetTextInputBaseHtmlTemplate()) { html, (token, value) ->
            html.replace("{{$token}}", value)
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

private const val WINDOWS_CLOSE_ICON =
    """<svg xmlns="http://www.w3.org/2000/svg" height="40px" viewBox="0 -960 960 960" width="40px" fill="currentColor"><path d="M480.67-404.67 373.99-298q-16.06 16.67-37.16 15.67-21.1-1-37.5-16-16-15-16-37t16-38l106-107.36-106.66-107.32q-15.34-15.23-15.34-36.58 0-21.35 15.84-37.41 15.17-15.33 37.25-15.33T373.67-662l107 106.67 105-106.67q15.4-16 37.83-15.67 22.44.34 38.5 15.67 13.67 15 13.67 36.67 0 21.66-13.67 37L555.33-480.67 662-371.99q15.33 15.56 15.33 36.91 0 21.34-15.33 36.9-16 14.51-38.33 14.85Q601.33-283 586-298L480.67-404.67Z"/></svg>"""
private const val WINDOWS_MIC_ICON =
    """<svg xmlns="http://www.w3.org/2000/svg" height="24" viewBox="0 -960 960 960" width="24" fill="currentColor"><path d="M480-400q-50 0-85-35t-35-85v-240q0-50 35-85t85-35q50 0 85 35t35 85v240q0 50-35 85t-85 35Zm-40 240v-83q-92-13-157.5-78T203-479q-2-17 9-29t28-12q17 0 28.5 11.5T284-480q14 70 69.5 115T480-320q72 0 127-45.5T676-480q4-17 15.5-28.5T720-520q17 0 28 12t9 29q-14 91-79 157t-158 79v83q0 17-11.5 28.5T480-120q-17 0-28.5-11.5T440-160Z"/></svg>"""
private const val WINDOWS_SEND_ICON =
    """<svg xmlns="http://www.w3.org/2000/svg" height="24px" viewBox="0 -960 960 960" width="24px" fill="currentColor"><path d="M176-183q-20 8-38-3.5T120-220v-180l320-80-320-80v-180q0-22 18-33.5t38-3.5l616 260q25 11 25 37t-25 37L176-183Z"/></svg>"""
