package dev.screengoated.toolbox.mobile.service.preset

import android.content.Context
import dev.screengoated.toolbox.mobile.service.overlay.overlayFontCss
import org.json.JSONObject

internal data class PresetTextInputHtmlSettings(
    val lang: String,
    val title: String,
    val placeholder: String,
    val isDark: Boolean,
    val owner: String,
)

internal class PresetTextInputHtmlBuilder(private val asset: (String) -> String) {
    constructor(context: Context) : this({ name ->
        context.assets.open("preset_overlay/$name").bufferedReader().use { it.readText() }
    })

    private val template by lazy { asset("windows_text_input.html") }
    private val operations by lazy { asset("windows_text_input_edits.js") }
    private val locales by lazy { JSONObject(asset("windows_text_input_locales.json")) }

    fun build(settings: PresetTextInputHtmlSettings): String {
        requireTextInputOwner(settings.owner)
        val copy = copy(settings.lang)
        val values = mapOf(
            "THEME_ATTR" to "data-theme=\"${if (settings.isDark) "dark" else "light"}\"",
            "FONT_CSS" to overlayFontCss(),
            "CSS" to css(settings.isDark),
            "TITLE_TEXT" to htmlEscape(settings.title),
            "PLACEHOLDER" to htmlEscape(settings.placeholder),
            "SUBMIT_TXT" to htmlEscape(copy.getString("text_input_footer_submit")),
            "NEWLINE_TXT" to htmlEscape(copy.getString("text_input_footer_newline")),
            "CANCEL_HINT" to "Esc",
            "CANCEL_TXT" to htmlEscape(copy.getString("text_input_footer_cancel")),
            "CLOSE_TITLE" to htmlEscape(copy.getString("text_input_close_tooltip")),
            "MIC_TITLE" to htmlEscape(copy.getString("text_input_speech_to_text_tooltip")),
            "SEND_TITLE" to htmlEscape(copy.getString("text_input_send_tooltip")),
            "BRIDGE_PRELUDE" to """
                window.__sgtDocumentNonce = ${scriptString(settings.owner)};
                window.__sgtCancelledThrough = '0';
                window.ipc = { postMessage(message) {
                    if (window.sgtAndroid && window.sgtAndroid.postMessage) window.sgtAndroid.postMessage(String(message));
                }};
            """.trimIndent(),
            "EDIT_OPERATIONS" to operations,
            "MOBILE_SHIM" to presetTextInputMobileShim(),
        )
        return Regex("\\{\\{([A-Z_]+)}}").replace(template) { match -> values.getValue(match.groupValues[1]) }
    }

    fun chromeScript(settings: PresetTextInputHtmlSettings): String {
        val copy = copy(settings.lang)
        val footer = "${copy.getString("text_input_footer_submit")}  |  ${copy.getString("text_input_footer_newline")}  |  Esc ${copy.getString("text_input_footer_cancel")}"
        return """
            if (window.__sgtTextSession === ${scriptString(settings.owner)} && !window.__sgtInputClosing) {
                document.getElementById('theme-style').textContent = ${scriptString(css(settings.isDark))};
                window.updateTheme(${settings.isDark});
                window.updateInputChrome(${scriptString(settings.owner)}, ${scriptString(settings.title)}, ${scriptString(footer)}, ${scriptString(settings.placeholder)});
            }
        """.trimIndent()
    }

    private fun css(dark: Boolean): String = asset("windows_text_input_${if (dark) "dark" else "light"}.css")
    private fun copy(lang: String): JSONObject = locales.optJSONObject(lang) ?: locales.getJSONObject("en")
    private fun htmlEscape(text: String): String = text.replace("&", "&amp;").replace("<", "&lt;")
        .replace(">", "&gt;").replace("\"", "&quot;")
}

internal fun scriptString(text: String): String = JSONObject.quote(text).replace("<", "\\u003c")
