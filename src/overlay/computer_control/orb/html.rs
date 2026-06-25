//! The Computer Control orb page: the canvas/JS asset (`orb.html`) with the
//! Google Sans Flex `@font-face` (served same-origin by the local font server)
//! injected so the caption text matches the app font. Material Symbols glyphs
//! load from the Google Fonts CDN (CC requires internet anyway).

/// Build the orb HTML to load into the overlay WebView.
pub(super) fn generate_orb_html() -> String {
    let font_css = crate::overlay::html_components::font_manager::get_font_css();
    let lang = crate::APP
        .lock()
        .ok()
        .map(|a| a.config.ui_language.clone())
        .unwrap_or_default();
    // Only a hint — typing itself works in EVERY language (the input has real keyboard focus, so the
    // OS IME composes Korean / Vietnamese / etc.). Localise the hint for the three UI languages.
    let placeholder = match lang.as_str() {
        "ko" => "명령 입력…",
        "vi" => "Nhập lệnh…",
        _ => "Type a command…",
    };
    include_str!("orb.html")
        .replace("/*FONT_CSS*/", &font_css)
        .replace("/*CMD_PLACEHOLDER*/", placeholder)
}
