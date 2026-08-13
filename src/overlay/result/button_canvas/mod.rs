//! Interactive controls rendered inside the unified result compositor.

mod actions;
mod css;
mod js;
mod js_refine;
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
        "send": icon("send"), "opacity": icon("opacity"), "close": icon("close"),
    })
    .to_string();
    [js::get_javascript(), js_refine::get_javascript()]
        .concat()
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

    #[test]
    fn opacity_updates_preserve_the_active_slider_element() {
        let script = super::js::get_javascript();
        let actions = include_str!("actions.rs");

        assert!(script.contains("const { opacityPercent, ...structuralState } = state;"));
        assert!(script.contains("opacity.value = opacityPercent;"));
        assert!(script.contains("JSON.stringify(structuralState)"));
        assert!(actions.contains("scene_compositor::set_control_scope_opacity(hwnd, value)"));
        assert!(!actions.contains("scene_compositor::sync_window"));
    }

    #[test]
    fn anchored_controls_keep_canonical_proximity_and_support_scaled_buttons() {
        let script = super::document_script();
        let css = super::document_css();

        assert!(script.contains("state.controlAnchor"));
        assert!(script.contains("state.controlScalePercent"));
        assert!(script.contains("placementRect.x + placementRect.w"));
        assert!(script.contains("e.button === 0 && groupActions"));
        assert!(script.contains("result_all_drag_start"));
        assert!(script.contains("window.previewResultDrag"));
        assert!(script.contains("card.style.translate = offset"));
        assert!(script.contains("function contrastingControlSurface"));
        assert!(script.contains("local-control-surface-light"));
        assert!(script.contains("local-control-surface-dark"));
        let drag_handler = script
            .split_once("function handleResultDrag")
            .expect("drag handler should exist")
            .1
            .split_once("window.previewResultDrag")
            .expect("drag preview should follow the pointer handler")
            .0;
        assert!(!drag_handler.contains(".style.opacity"));
        assert!(script.contains("action('${hwnd}', 'copy')"));
        assert!(script.contains("action('${hwnd}', 'download')"));
        assert!(script.contains("action('${hwnd}', 'speaker')"));
        assert!(script.contains("const maxRadius = 150"));
        assert!(css.contains("--control-scale: 1"));
        assert!(css.contains("calc(24px * var(--control-scale))"));
        assert!(css.contains("calc(6px * var(--control-scale))"));
        assert!(css.contains("calc(12px * var(--control-scale))"));
        assert!(css.contains("calc(9px * var(--control-scale))"));
        assert!(css.contains(".button-group.local-control-surface-light"));
        assert!(css.contains(".button-group.local-control-surface-dark"));
        assert!(!script.contains("dismiss_chain"));
        assert!(!script.contains("copy_all"));
    }
}
