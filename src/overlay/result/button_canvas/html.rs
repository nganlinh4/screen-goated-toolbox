//! HTML template generation for button canvas

use super::css::get_base_css;
use super::js::get_javascript;
use super::theme::get_canvas_theme_css;
use super::LAST_THEME_IS_DARK;
use std::sync::atomic::Ordering;

/// Generate the complete HTML for the canvas WebView
pub fn generate_canvas_html() -> String {
    let font_css = crate::overlay::html_components::font_manager::get_font_css();

    // Get localization
    let lang = crate::APP.lock().unwrap().config.ui_language.clone();
    let locale = crate::gui::locale::LocaleText::get(&lang);
    let l10n_json = serde_json::json!({
        "copy": locale.overlay_copy_tooltip,
        "undo": locale.overlay_undo_tooltip,
        "redo": locale.overlay_redo_tooltip,
        "edit": locale.overlay_edit_tooltip,
        "markdown": locale.overlay_markdown_tooltip,
        "download": locale.overlay_download_tooltip,
        "speaker": locale.overlay_speaker_tooltip,
        "broom": locale.overlay_broom_tooltip,
        "back": locale.overlay_back_tooltip,
        "forward": locale.overlay_forward_tooltip,
        "opacity": locale.overlay_opacity_tooltip,
        "overlay_refine_placeholder": locale.overlay_refine_placeholder,
    })
    .to_string();

    let is_dark = crate::overlay::is_dark_mode();

    // Initialize state
    LAST_THEME_IS_DARK.store(is_dark, Ordering::SeqCst);
    let theme_css = get_canvas_theme_css(is_dark);

    // Get icon SVGs with theme-appropriate colors
    let get_colored_svg = |name: &str| -> String {
        crate::overlay::html_components::icons::get_icon_svg(name).to_string()
    };

    let icon_svgs_json = serde_json::json!({
        "arrow_back": get_colored_svg("arrow_back"),
        "arrow_forward": get_colored_svg("arrow_forward"),
        "undo": get_colored_svg("undo"),
        "redo": get_colored_svg("redo"),
        "newsmode": get_colored_svg("newsmode"),
        "notes": get_colored_svg("notes"),
        "hourglass_empty": get_colored_svg("hourglass_empty"),
        "stop": get_colored_svg("stop"),
        "cleaning_services": get_colored_svg("cleaning_services"),
        "content_copy": get_colored_svg("content_copy"),
        "check": get_colored_svg("check"),
        "download": get_colored_svg("download"),
        "volume_up": get_colored_svg("volume_up"),
        "mic": get_colored_svg("mic"),
        "send": get_colored_svg("send"),
        "opacity": get_colored_svg("opacity"),
    })
    .to_string();

    format!(
        r#"<!DOCTYPE html>
<html>
<head>
<meta charset="UTF-8">
<style>
{font_css}
</style>
<style id="theme-css">
{theme_css}
</style>
<style>
{base_css}
</style>
</head>
<body>
<div id="button-container"></div>
<script>
{javascript}
</script>
</body>
</html>"#,
        font_css = font_css,
        theme_css = theme_css,
        base_css = get_base_css(),
        javascript = get_javascript(),
    )
    .replace("#L10N_JSON#", &l10n_json)
    .replace("#ICON_SVGS_JSON#", &icon_svgs_json)
}
