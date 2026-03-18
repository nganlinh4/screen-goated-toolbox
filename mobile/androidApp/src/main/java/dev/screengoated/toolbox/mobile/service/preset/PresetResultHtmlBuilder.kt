package dev.screengoated.toolbox.mobile.service.preset

import android.content.Context
import dev.screengoated.toolbox.mobile.service.overlay.overlayFontCss
import org.json.JSONObject

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
    private val gridInitScript by lazy { asset("windows_gridjs_init.js") }
    val m3eCacheDir: java.io.File by lazy {
        val dir = context.cacheDir.resolve("m3e_scripts")
        dir.mkdirs()
        val libFile = dir.resolve("m3e_loading_indicator.js")
        val initFile = dir.resolve("m3e_loading_init.js")
        if (!libFile.exists() || libFile.length() == 0L) {
            libFile.writeText(asset("m3e_loading_indicator.js") + "\nwindow.RoundedPolygon=RoundedPolygon;window.Morph=Morph;\n")
        }
        if (!initFile.exists() || initFile.length() == 0L) {
            initFile.writeText(asset("m3e_loading_init.js"))
        }
        dir
    }

    private val gridUrls by lazy {
        JSONObject(asset("windows_gridjs_urls.json")).let { payload ->
            payload.getString("cssUrl") to payload.getString("jsUrl")
        }
    }

    fun build(settings: PresetResultHtmlSettings): String {
        val replacements = linkedMapOf(
            "FONT_CSS" to overlayFontCss(),
            "THEME_CSS" to if (settings.isDark) themeCssDark else themeCssLight,
            "WINDOW_CHROME_CSS" to presetResultCss(settings.isDark),
            "MARKDOWN_CSS" to markdownCss,
            "GRIDJS_CSS_URL" to gridUrls.first,
            "GRIDJS_JS_URL" to gridUrls.second,
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
