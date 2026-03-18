package dev.screengoated.toolbox.mobile.service.preset

import android.content.Context
import dev.screengoated.toolbox.mobile.service.overlay.overlayFontCss

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

    fun build(settings: PresetButtonCanvasHtmlSettings): String {
        val replacements = linkedMapOf(
            "FONT_CSS" to overlayFontCss(),
            "THEME_CSS" to if (settings.isDark) darkThemeCss else lightThemeCss,
            "BASE_CSS" to canvasCss,
            "CANVAS_JS" to canvasJs
                .replace("#L10N_JSON#", buttonCanvasL10n(settings.lang))
                .replace("#ICON_SVGS_JSON#", buttonCanvasIconsJson()),
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

private fun buttonCanvasIconsJson(): String {
    val icons = mapOf(
        "arrow_back" to windowsCanvasIconSvg("arrow_back"),
        "arrow_forward" to windowsCanvasIconSvg("arrow_forward"),
        "undo" to windowsCanvasIconSvg("undo"),
        "redo" to windowsCanvasIconSvg("redo"),
        "newsmode" to windowsCanvasIconSvg("newsmode"),
        "notes" to windowsCanvasIconSvg("notes"),
        "hourglass_empty" to windowsCanvasIconSvg("hourglass_empty"),
        "stop" to windowsCanvasIconSvg("stop"),
        "cleaning_services" to windowsCanvasIconSvg("cleaning_services"),
        "content_copy" to windowsCanvasIconSvg("content_copy"),
        "check" to windowsCanvasIconSvg("check"),
        "download" to windowsCanvasIconSvg("download"),
        "volume_up" to windowsCanvasIconSvg("volume_up"),
        "mic" to windowsCanvasIconSvg("mic"),
        "send" to windowsCanvasIconSvg("send"),
        "opacity" to windowsCanvasIconSvg("opacity"),
    )
    return org.json.JSONObject(icons).toString()
}

private fun localize(lang: String, en: String, vi: String, ko: String): String = when (lang) {
    "vi" -> vi
    "ko" -> ko
    else -> en
}

private fun windowsCanvasIconSvg(name: String): String = when (name) {
    "mic" ->
        """<svg xmlns="http://www.w3.org/2000/svg" height="24" viewBox="0 -960 960 960" width="24" fill="currentColor"><path d="M480-400q-50 0-85-35t-35-85v-240q0-50 35-85t85-35q50 0 85 35t35 85v240q0 50-35 85t-85 35Zm-40 240v-83q-92-13-157.5-78T203-479q-2-17 9-29t28-12q17 0 28.5 11.5T284-480q14 70 69.5 115T480-320q72 0 127-45.5T676-480q4-17 15.5-28.5T720-520q17 0 28 12t9 29q-14 91-79 157t-158 79v83q0 17-11.5 28.5T480-120q-17 0-28.5-11.5T440-160Z"/></svg>"""
    "volume_up" ->
        """<svg xmlns="http://www.w3.org/2000/svg" height="16" viewBox="0 -960 960 960" width="16" fill="currentColor"><path d="M760-481q0-83-44-151.5T598-735q-15-7-22-21.5t-2-29.5q6-16 21.5-23t31.5 0q97 43 155 131.5T840-481q0 108-58 196.5T627-153q-16 7-31.5 0T574-176q-5-15 2-29.5t22-21.5q74-34 118-102.5T760-481ZM280-360H160q-17 0-28.5-11.5T120-400v-160q0-17 11.5-28.5T160-600h120l132-132q19-19 43.5-8.5T480-703v446q0 27-24.5 37.5T412-228L280-360Zm380-120q0 42-19 79.5T591-339q-10 6-20.5.5T560-356v-250q0-12 10.5-17.5t20.5.5q31 25 50 63t19 80Z"/></svg>"""
    "content_copy" ->
        """<svg xmlns="http://www.w3.org/2000/svg" height="16" viewBox="0 -960 960 960" width="16" fill="currentColor"><path d="M360-240q-33 0-56.5-23.5T280-320v-480q0-33 23.5-56.5T360-880h360q33 0 56.5 23.5T800-800v480q0 33-23.5 56.5T720-240H360ZM200-80q-33 0-56.5-23.5T120-160v-520q0-17 11.5-28.5T160-720q17 0 28.5 11.5T200-680v520h400q17 0 28.5 11.5T640-120q0 17-11.5 28.5T600-80H200Z"/></svg>"""
    "check" ->
        """<svg xmlns="http://www.w3.org/2000/svg" height="16" viewBox="0 -960 960 960" width="16" fill="currentColor"><path d="m382-354 339-339q12-12 28-12t28 12q12 12 12 28.5T777-636L410-268q-12 12-28 12t-28-12L182-440q-12-12-11.5-28.5T183-497q12-12 28.5-12t28.5 12l142 143Z"/></svg>"""
    "download" ->
        """<svg xmlns="http://www.w3.org/2000/svg" height="16" viewBox="0 -960 960 960" width="16" fill="currentColor"><path d="M480-320 280-520l56-58 104 104v-326h80v326l104-104 56 58-200 200ZM240-160q-33 0-56.5-23.5T160-240v-120h80v120h480v-120h80v120q0 33-23.5 56.5T720-160H240Z"/></svg>"""
    "send" ->
        """<svg xmlns="http://www.w3.org/2000/svg" height="24px" viewBox="0 -960 960 960" width="24px" fill="currentColor"><path d="M176-183q-20 8-38-3.5T120-220v-180l320-80-320-80v-180q0-22 18-33.5t38-3.5l616 260q25 11 25 37t-25 37L176-183Z"/></svg>"""
    "arrow_back" ->
        """<svg xmlns="http://www.w3.org/2000/svg" height="16px" viewBox="0 -960 960 960" width="16px" fill="currentColor"><path d="m313-440 196 196q12 12 11.5 28T508-188q-12 11-28 11.5T452-188L188-452q-6-6-8.5-13t-2.5-15q0-8 2.5-15t8.5-13l264-264q11-11 27.5-11t28.5 11q12 12 12 28.5T508-715L313-520h447q17 0 28.5 11.5T800-480q0 17-11.5 28.5T760-440H313Z"/></svg>"""
    "arrow_forward" ->
        """<svg xmlns="http://www.w3.org/2000/svg" height="16px" viewBox="0 -960 960 960" width="16px" fill="currentColor"><path d="M647-440H200q-17 0-28.5-11.5T160-480q0-17 11.5-28.5T200-520h447L451-716q-12-12-11.5-28t12.5-28q12-11 28-11.5t28 11.5l264 264q6 6 8.5 13t2.5 15q0 8-2.5 15t-8.5 13L508-188q-11 11-27.5 11T452-188q-12-12-12-28.5t12-28.5l195-195Z"/></svg>"""
    "undo" ->
        """<svg xmlns="http://www.w3.org/2000/svg" height="16px" viewBox="0 -960 960 960" width="16px" fill="currentColor"><path d="M320-200q-17 0-28.5-11.5T280-240q0-17 11.5-28.5T320-280h244q63 0 109.5-40T720-420q0-60-46.5-100T564-560H312l76 76q11 11 11 28t-11 28q-11 11-28 11t-28-11L188-572q-6-6-8.5-13t-2.5-15q0-8 2.5-15t8.5-13l144-144q11-11 28-11t28 11q11 11 11 28t-11 28l-76 76h252q97 0 166.5 63T800-420q0 94-69.5 157T564-200H320Z"/></svg>"""
    "redo" ->
        """<svg xmlns="http://www.w3.org/2000/svg" height="16px" viewBox="0 -960 960 960" width="16px" fill="currentColor"><path d="M648-560H396q-63 0-109.5 40T240-420q0 60 46.5 100T396-280h244q17 0 28.5 11.5T680-240q0 17-11.5 28.5T640-200H396q-97 0-166.5-63T160-420q0-94 69.5-157T396-640h252l-76-76q-11-11-11-28t11-28q11-11 28-11t28 11l144 144q6 6 8.5 13t2.5 15q0 8-2.5 15t-8.5 13L628-428q-11 11-28 11t-28-11q-11-11-11-28t11-28l76-76Z"/></svg>"""
    "newsmode" ->
        """<svg xmlns="http://www.w3.org/2000/svg" height="16px" viewBox="0 -960 960 960" width="16px" fill="currentColor"><path d="M160-120q-33 0-56.5-23.5T80-200v-560q0-33 23.5-56.5T160-840h640q33 0 56.5 23.5T880-760v560q0 33-23.5 56.5T800-120H160Zm0-80h640v-560H160v560Zm120-80h400q17 0 28.5-11.5T720-320q0-17-11.5-28.5T680-360H280q-17 0-28.5 11.5T240-320q0 17 11.5 28.5T280-280Zm0-160h80q17 0 28.5-11.5T400-480v-160q0-17-11.5-28.5T360-680h-80q-17 0-28.5 11.5T240-640v160q0 17 11.5 28.5T280-440Zm240 0h160q17 0 28.5-11.5T720-480q0-17-11.5-28.5T680-520H520q-17 0-28.5 11.5T480-480q0 17 11.5 28.5T520-440Zm0-160h160q17 0 28.5-11.5T720-640q0-17-11.5-28.5T680-680H520q-17 0-28.5 11.5T480-640q0 17 11.5 28.5T520-600ZM160-200v-560 560Z"/></svg>"""
    "notes" ->
        """<svg xmlns="http://www.w3.org/2000/svg" height="16px" viewBox="0 -960 960 960" width="16px" fill="currentColor"><path d="M160-240q-17 0-28.5-11.5T120-280q0-17 11.5-28.5T160-320h400q17 0 28.5 11.5T600-280q0 17-11.5 28.5T560-240H160Zm0-200q-17 0-28.5-11.5T120-480q0-17 11.5-28.5T160-520h640q17 0 28.5 11.5T840-480q0 17-11.5 28.5T800-440H160Zm0-200q-17 0-28.5-11.5T120-680q0-17 11.5-28.5T160-720h640q17 0 28.5 11.5T840-680q0 17-11.5 28.5T800-640H160Z"/></svg>"""
    "hourglass_empty" ->
        """<svg xmlns="http://www.w3.org/2000/svg" height="16px" viewBox="0 -960 960 960" width="16px" fill="currentColor"><path d="M320-160h320v-120q0-66-47-113t-113-47q-66 0-113 47t-47 113v120Zm160-360q66 0 113-47t47-113v-120H320v120q0 66 47 113t113 47ZM200-80q-17 0-28.5-11.5T160-120q0-17 11.5-28.5T200-160h40v-120q0-61 28.5-114.5T348-480q-51-32-79.5-85.5T240-680v-120h-40q-17 0-28.5-11.5T160-840q0-17 11.5-28.5T200-880h560q17 0 28.5 11.5T800-840q0 17-11.5 28.5T760-800h-40v120q0 61-28.5 114.5T612-480q51 32 79.5 85.5T720-280v120h40q17 0 28.5 11.5T800-120q0 17-11.5 28.5T760-80H200Z"/></svg>"""
    "stop" ->
        """<svg xmlns="http://www.w3.org/2000/svg" height="16px" viewBox="0 -960 960 960" width="16px" fill="currentColor"><path d="M240-320v-320q0-33 23.5-56.5T320-720h320q33 0 56.5 23.5T720-640v320q0 33-23.5 56.5T640-240H320q-33 0-56.5-23.5T240-320Zm80 0h320v-320H320v320Zm160-160Z"/></svg>"""
    "cleaning_services" ->
        """<svg xmlns="http://www.w3.org/2000/svg" height="16px" viewBox="0 -960 960 960" width="16px" fill="currentColor"><path d="M440-520h80v-280q0-17-11.5-28.5T480-840q-17 0-28.5 11.5T440-800v280ZM200-360h560v-80H200v80Zm-58 240h98v-80q0-17 11.5-28.5T280-240q17 0 28.5 11.5T320-200v80h120v-80q0-17 11.5-28.5T480-240q17 0 28.5 11.5T520-200v80h120v-80q0-17 11.5-28.5T680-240q17 0 28.5 11.5T720-200v80h98l-40-160H182l-40 160Zm676 80H142q-39 0-63-31t-14-69l55-220v-80q0-33 23.5-56.5T200-520h160v-280q0-50 35-85t85-35q50 0 85 35t35 85v280h160q33 0 56.5 23.5T840-440v80l55 220q13 38-11.5 69T818-40Zm-58-400H200h560Zm-240-80h-80 80Z"/></svg>"""
    "opacity" ->
        """<svg xmlns="http://www.w3.org/2000/svg" height="16px" viewBox="0 -960 960 960" width="16px" fill="currentColor"><path d="M480-120q-133 0-226.5-92T160-436q0-65 25-121.5T254-658l226-222 226 222q44 44 69 100.5T800-436q0 132-93.5 224T480-120ZM242-400h474q12-72-13.5-123T650-600L480-768 310-600q-27 26-53 77t-15 123Z"/></svg>"""
    else -> ""
}
