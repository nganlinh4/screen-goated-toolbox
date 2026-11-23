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
    private val themeCssDark by lazy { asset("windows_markdown_theme_dark.css") }
    private val themeCssLight by lazy { asset("windows_markdown_theme_light.css") }
    private val fitScript by lazy { asset("windows_markdown_fit.js") }
    private val gridCss by lazy { asset("windows_gridjs.css") }
    private val gridLibCss by lazy { asset("windows_gridjs_lib.css") }
    private val gridLibJs by lazy { asset("windows_gridjs_lib.js") }
    private val resultDomPatch by lazy { asset("windows_result_dom_patch.js") }
    private val gridInitScript by lazy { asset("windows_gridjs_init.js") }
    private val loadingLib by lazy {
        asset("m3e_loading_indicator_1.js") + asset("m3e_loading_indicator_2.js")
    }
    private val loadingInit by lazy { asset("m3e_loading_init.js") }

    fun build(settings: PresetResultHtmlSettings): String {
        val replacements = linkedMapOf(
            "FONT_CSS" to overlayFontCss(),
            "THEME_CSS" to if (settings.isDark) themeCssDark else themeCssLight,
            "WINDOW_CHROME_CSS" to presetResultCss(settings.isDark),
            "MARKDOWN_CSS" to markdownCss,
            "GRIDJS_LIB_CSS" to gridLibCss,
            "GRIDJS_LIB_JS" to gridLibJs,
            "RESULT_DOM_PATCH_JS" to resultDomPatch,
            "M3E_LIB_JS" to loadingLib,
            "M3E_INIT_JS" to loadingInit,
            "M3E_IS_DARK" to settings.isDark.toString(),
            "GRIDJS_CSS" to gridCss,
            "GRIDJS_INIT_SCRIPT" to gridInitScript,
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
