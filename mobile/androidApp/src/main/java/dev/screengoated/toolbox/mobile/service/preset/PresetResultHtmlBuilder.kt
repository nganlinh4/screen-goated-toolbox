package dev.screengoated.toolbox.mobile.service.preset

import android.content.Context
import dev.screengoated.toolbox.mobile.service.overlay.overlayFontCss

internal data class PresetResultHtmlSettings(
    val isDark: Boolean,
)

internal class PresetResultHtmlBuilder(
    private val context: Context,
) {
    private val markdownCss by lazy { asset("windows_markdown.css") }
    private val fitScript by lazy { asset("windows_markdown_fit.js") }

    fun build(settings: PresetResultHtmlSettings): String {
        val replacements = linkedMapOf(
            "FONT_CSS" to overlayFontCss(),
            "RESULT_CSS" to presetResultCss(settings.isDark),
            "MARKDOWN_CSS" to markdownCss,
            "FIT_SCRIPT" to fitScript,
            "RESULT_JS" to presetResultJavascript(),
        )
        return replacements.entries.fold(presetResultBaseHtmlTemplate()) { html, (token, value) ->
            html.replace("{{$token}}", value)
        }
    }

    private fun asset(name: String): String {
        return context.assets.open("preset_overlay/$name").bufferedReader().use { it.readText() }
    }
}
