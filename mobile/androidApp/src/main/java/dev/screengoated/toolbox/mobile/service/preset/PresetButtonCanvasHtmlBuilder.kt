package dev.screengoated.toolbox.mobile.service.preset

import android.content.Context
import dev.screengoated.toolbox.mobile.preset.triLang
import dev.screengoated.toolbox.mobile.service.overlay.overlayFontCss
import dev.screengoated.toolbox.mobile.ui.icons.MaterialSymbolSvgs

internal data class PresetButtonCanvasHtmlSettings(
    val lang: String,
    val isDark: Boolean,
)

internal class PresetButtonCanvasHtmlBuilder(
    private val context: Context,
) {
    private val canvasCss by lazy { asset("windows_button_canvas.css") }
    private val canvasJs by lazy { patchedJs(asset("windows_button_canvas.js")) }
    private val darkThemeCss by lazy { asset("windows_button_canvas_theme_dark.css") }
    private val lightThemeCss by lazy { asset("windows_button_canvas_theme_light.css") }
    private val iconsJson by lazy { buttonCanvasIconsJson(MaterialSymbolSvgs.fromAssets(context)) }

    fun build(settings: PresetButtonCanvasHtmlSettings): String {
        val replacements = linkedMapOf(
            "FONT_CSS" to overlayFontCss(),
            "THEME_CSS" to if (settings.isDark) darkThemeCss else lightThemeCss,
            "BASE_CSS" to canvasCss,
            "CANVAS_JS" to canvasJs
                .replace("#L10N_JSON#", buttonCanvasL10n(settings.lang))
                .replace("#ICON_SVGS_JSON#", iconsJson),
            "MOBILE_CANVAS_JS" to mobileCanvasJavascript(),
        )
        return replacements.entries.fold(presetButtonCanvasBaseHtmlTemplate()) { html, (token, value) ->
            html.replace("{{$token}}", value)
        }
    }

    private fun asset(name: String): String {
        return context.assets.open("preset_overlay/$name").bufferedReader().use { it.readText() }
    }

    private fun patchedJs(source: String): String {
        return source
            .replace("onclick=\"action('${'$'}{hwnd}', 'back')\"", "data-action=\"back\" onclick=\"action('${'$'}{hwnd}', 'back')\"")
            .replace("onclick=\"action('${'$'}{hwnd}', 'forward')\"", "data-action=\"forward\" onclick=\"action('${'$'}{hwnd}', 'forward')\"")
            .replace("onclick=\"action('${'$'}{hwnd}', 'copy')\"", "data-action=\"copy\" onclick=\"action('${'$'}{hwnd}', 'copy')\"")
            .replace("onclick=\"action('${'$'}{hwnd}', 'undo')\"", "data-action=\"undo\" onclick=\"action('${'$'}{hwnd}', 'undo')\"")
            .replace("onclick=\"action('${'$'}{hwnd}', 'redo')\"", "data-action=\"redo\" onclick=\"action('${'$'}{hwnd}', 'redo')\"")
            .replace("onclick=\"action('${'$'}{hwnd}', 'edit')\"", "data-action=\"edit\" onclick=\"action('${'$'}{hwnd}', 'edit')\"")
            .replace("onclick=\"action('${'$'}{hwnd}', 'markdown')\"", "data-action=\"markdown\" onclick=\"action('${'$'}{hwnd}', 'markdown')\"")
            .replace("onclick=\"action('${'$'}{hwnd}', 'download')\"", "data-action=\"download\" onclick=\"action('${'$'}{hwnd}', 'download')\"")
            .replace("onclick=\"action('${'$'}{hwnd}', 'speaker')\"", "data-action=\"speaker\" onclick=\"action('${'$'}{hwnd}', 'speaker')\"")
            .replace("class=\"btn broom\"", "class=\"btn broom\" data-action=\"broom_click\"")
    }
}

private fun buttonCanvasL10n(lang: String): String {
    val map = mapOf(
        "copy" to localize(lang, "Copy", "Sao chép", "복사"),
        "undo" to localize(lang, "Undo", "Hoàn tác", "실행 취소"),
        "redo" to localize(lang, "Redo", "Làm lại", "다시 실행"),
        "edit" to localize(lang, "Edit / Refine", "Chỉnh sửa / Viết lại", "편집 / 다듬기"),
        "markdown" to localize(lang, "Toggle Markdown", "Bật/Tắt Markdown", "마크다운 토글"),
        "download" to localize(lang, "Save HTML", "Tải về HTML", "HTML 저장"),
        "speaker" to localize(lang, "Speak (TTS)", "Đọc to (TTS)", "텍스트 읽기 (TTS)"),
        "broom" to localize(lang, "Dismiss", "Đóng", "닫기"),
        "back" to localize(lang, "Back", "Quay lại", "뒤로"),
        "forward" to localize(lang, "Forward", "Tiếp theo", "앞으로"),
        "opacity" to localize(lang, "Opacity", "Độ mờ", "불투명도"),
        "overlay_refine_placeholder" to localize(lang, "Refine result...", "Chỉnh sửa kết quả...", "결과 수정..."),
    )
    return org.json.JSONObject(map).toString()
}

/**
 * Symbols keyed as the canvas script reads them (`window.iconSvgs`), at the pixel size each
 * button had on Windows; `filter` keeps the catalog `1em` like the Windows control surface.
 */
internal fun buttonCanvasIconsJson(symbols: MaterialSymbolSvgs): String {
    val icons = BUTTON_CANVAS_ICONS_16PX.associateWith { symbols.svg(it, sizePx = 16) } +
        ("filter" to symbols.svg("filter"))
    return org.json.JSONObject(icons).toString()
}

private val BUTTON_CANVAS_ICONS_16PX = listOf(
    "arrow_back",
    "arrow_forward",
    "undo",
    "redo",
    "newsmode",
    "notes",
    "hourglass_empty",
    "stop",
    "cleaning_services",
    "content_copy",
    "check",
    "download",
    "volume_up",
    "opacity",
)

private fun localize(lang: String, en: String, vi: String, ko: String): String = triLang(lang, en, vi, ko)
