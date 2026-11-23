use super::{
    WM_NAVIGATE, WM_SYNC, content_bounds, host_alpha, navigation_initialization_script,
    navigation_surface_active, navigation_view, public_web_url, should_activate_foreground,
};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::WM_APP;

#[test]
fn external_navigation_sync_uses_a_private_window_message() {
    assert!(std::hint::black_box(WM_SYNC) > WM_APP);
    assert!(std::hint::black_box(WM_NAVIGATE) > WM_SYNC);
}

#[test]
fn browsing_accepts_only_absolute_public_web_urls() {
    assert_eq!(
        public_web_url("https://example.com/page").as_deref(),
        Some("https://example.com/page")
    );
    assert!(public_web_url("javascript:alert(1)").is_none());
    assert!(public_web_url("/relative").is_none());
}

#[test]
fn real_pointer_interaction_activates_keyboard_focus_but_offscreen_acceptance_does_not() {
    assert!(should_activate_foreground(false));
    assert!(!should_activate_foreground(true));
}

#[test]
fn external_navigation_uses_the_complete_result_surface() {
    let bounds = content_bounds(HWND::default());
    let wry::dpi::Position::Physical(position) = bounds.position else {
        panic!("navigation bounds must use physical pixels");
    };
    assert_eq!((position.x, position.y), (0, 0));
}

#[test]
fn authored_result_mode_never_exposes_a_stored_navigation_surface() {
    assert!(!navigation_surface_active(true, false, 1, false));
    assert!(!navigation_surface_active(true, true, 0, false));
    assert!(!navigation_surface_active(true, true, 1, true));
    assert!(navigation_surface_active(true, true, 1, false));
}

#[test]
fn external_navigation_is_fully_opaque_instead_of_inheriting_result_dimming() {
    assert_eq!(host_alpha(true), 255);
    assert_eq!(host_alpha(false), 1);
}

#[test]
fn native_page_stays_hidden_until_its_top_level_load_finishes() {
    let loading = navigation_view(1, true);
    assert!(loading.browsing && loading.loading && !loading.visible);
    let loaded = navigation_view(1, false);
    assert!(loaded.browsing && !loaded.loading && loaded.visible);
    // Back at the authored result, a stale load flag never shows the page.
    let result = navigation_view(0, true);
    assert!(!result.browsing && !result.loading && !result.visible);
}

#[test]
fn remote_navigation_has_no_privileged_ipc_and_cannot_activate_on_load() {
    let source = include_str!("../raw_webview.rs");
    let builder = source
        .split("fn build_navigation(")
        .nth(1)
        .unwrap()
        .split("pub(super) fn navigate_pending(")
        .next()
        .unwrap();
    assert!(!builder.contains(".with_ipc_handler("));
    assert!(builder.contains("navigation_events::attach(&webview, hwnd)"));
    assert!(builder.contains("wry::NewWindowResponse::Deny"));
    assert!(builder.contains(".with_download_started_handler(|_, _| false)"));
    let presentation = source
        .split("fn present_navigation(")
        .nth(1)
        .unwrap()
        .split("pub(super) fn focus(")
        .next()
        .unwrap();
    assert!(!presentation.contains("focus(hwnd)"));
}

#[test]
fn external_navigation_page_script_does_not_own_overlay_geometry() {
    let script = navigation_initialization_script();
    assert!(!script.contains("resize"));
    assert!(!script.contains("scrollbar"));
    assert!(!script.contains("anchor.target === '_blank'"));
}
