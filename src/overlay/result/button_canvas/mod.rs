//! Interactive controls rendered inside the unified result compositor.

mod actions;
mod css;
mod js;
mod theme;

use windows::Win32::Foundation::HWND;

pub(crate) fn document_css() -> &'static str {
    css::get_base_css()
}

pub(crate) fn document_script() -> String {
    let lang = crate::APP.lock().unwrap().config.ui_language.clone();
    let locale = crate::gui::locale::LocaleText::get(&lang);
    let l10n = serde_json::json!({
        "copy": locale.overlay.overlay_copy_tooltip,
        "undo": locale.overlay.overlay_undo_tooltip,
        "redo": locale.overlay.overlay_redo_tooltip,
        "edit": locale.overlay.overlay_edit_tooltip,
        "download": locale.overlay.overlay_download_tooltip,
        "speaker": locale.overlay.overlay_speaker_tooltip,
        "result_handle": locale.overlay.overlay_result_handle_tooltip,
        "back": locale.overlay.overlay_back_tooltip,
        "forward": locale.overlay.overlay_forward_tooltip,
        "opacity": locale.overlay.overlay_opacity_tooltip,
        "cancel": locale.overlay.overlay_cancel_tooltip,
        "overlay_refine_placeholder": locale.overlay.overlay_refine_placeholder,
    })
    .to_string();
    let icon = |name| crate::overlay::html_components::icons::get_icon_svg(name).to_string();
    let icons = serde_json::json!({
        "arrow_back": icon("arrow_back"), "arrow_forward": icon("arrow_forward"),
        "undo": icon("undo"), "redo": icon("redo"),
        "hourglass_empty": icon("hourglass_empty"), "stop": icon("stop"),
        "cleaning_services": icon("cleaning_services"), "content_copy": icon("content_copy"),
        "check": icon("check"), "download": icon("download"),
        "volume_up": icon("volume_up"), "mic": icon("mic"),
        "send": icon("send"), "opacity": icon("opacity"),
    })
    .to_string();
    js::get_javascript()
        .replace("#L10N_JSON#", &l10n)
        .replace("#ICON_SVGS_JSON#", &icons)
}

pub(crate) fn theme_css(is_dark: bool) -> String {
    theme::get_canvas_theme_css(is_dark)
}

pub fn update_window_position(hwnd: HWND) {
    crate::overlay::result::scene_compositor::sync_controls(hwnd);
}

pub fn update_canvas() {
    crate::overlay::result::scene_compositor::sync_all_controls();
}

pub fn send_refine_text_update(hwnd: HWND, text: &str, is_insert: bool) {
    crate::overlay::result::scene_compositor::set_refine_text(hwnd, text, is_insert);
}

pub fn is_dragging() -> bool {
    crate::overlay::result::scene_compositor::is_dragging()
}

pub fn is_point_over_result_window(x: i32, y: i32) -> bool {
    crate::overlay::result::scene_compositor::is_point_over_result_window(x, y)
}

pub fn set_drag_mode(active: bool) {
    crate::overlay::result::scene_compositor::set_external_drag(active);
}

pub(crate) fn handle_action(
    id: isize,
    action: crate::overlay::result::scene_compositor::protocol::ButtonAction,
) {
    actions::handle(id, action);
}

pub(crate) fn handle_drag_finished(
    id: isize,
    targets: &[isize],
    outcome: crate::overlay::result::scene_compositor::protocol::DragOutcome,
) {
    actions::handle_drag_finished(id, targets, outcome);
}

#[cfg(test)]
mod tests {
    #[test]
    fn result_cards_and_controls_own_exactly_one_webview_builder() {
        let compositor = include_str!("../scene_compositor/child.rs");
        let controls = include_str!("mod.rs");

        assert_eq!(compositor.matches("WebViewBuilder::").count(), 1);
        assert!(!controls.contains(&["create_canvas", "_window"].concat()));
        assert!(!controls.contains(&["Web", "Context"].concat()));
    }
}
