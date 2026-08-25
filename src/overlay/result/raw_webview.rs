use crate::win_types::HwndWrapper;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::{LazyLock, Mutex};
use windows::Win32::Foundation::{COLORREF, HWND, RECT};
use windows::Win32::Graphics::Dwm::{
    DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND, DWMWCP_ROUND, DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Gdi::SetWindowRgn;
use windows::Win32::UI::WindowsAndMessaging::{
    GWL_STYLE, GetClientRect, GetWindowLongPtrW, HWND_TOPMOST, IsWindowVisible, LWA_ALPHA,
    PostMessageW, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetLayeredWindowAttributes,
    SetWindowLongPtrW, SetWindowPos, WM_APP, WS_CLIPCHILDREN,
};
use wry::{PageLoadEvent, Rect, WebView, WebViewBuilder};

pub(super) const WM_SYNC: u32 = WM_APP + 241;
pub(super) const WM_NAVIGATE: u32 = WM_APP + 242;
static ACTIVE: LazyLock<Mutex<HashSet<isize>>> = LazyLock::new(|| Mutex::new(HashSet::new()));
static EARLY_FINISHED_LOADS: LazyLock<Mutex<HashSet<isize>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));
static PENDING_NAVIGATION: LazyLock<Mutex<HashMap<isize, String>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

struct RawView {
    webview: WebView,
    urls: Vec<String>,
    depth: usize,
    loading: bool,
}

thread_local! {
    static VIEWS: RefCell<HashMap<isize, RawView>> = RefCell::new(HashMap::new());
    static CONTEXT: RefCell<Option<crate::overlay::webview_runtime::ManagedContext>> = const { RefCell::new(None) };
}

pub(super) fn request_sync(hwnd: HWND) {
    unsafe {
        let _ = PostMessageW(Some(hwnd), WM_SYNC, Default::default(), Default::default());
    }
}

pub(in crate::overlay::result) fn request_navigation(hwnd: HWND, raw_url: &str) {
    let Some(url) = public_web_url(raw_url) else {
        return;
    };
    PENDING_NAVIGATION
        .lock()
        .unwrap()
        .insert(hwnd.0 as isize, url);
    unsafe {
        let _ = PostMessageW(
            Some(hwnd),
            WM_NAVIGATE,
            Default::default(),
            Default::default(),
        );
    }
}

pub(super) fn is_active(hwnd: HWND) -> bool {
    ACTIVE.lock().unwrap().contains(&(hwnd.0 as isize))
}

pub(super) fn sync(hwnd: HWND) {
    let key = hwnd.0 as isize;
    let snapshot = super::WINDOW_STATES
        .lock()
        .unwrap()
        .get(&key)
        .map(|state| (state.is_browsing, state.is_navigation_loading));
    let Some((browsing, loading)) = snapshot else {
        destroy(hwnd);
        return;
    };
    let visible = unsafe { IsWindowVisible(hwnd).as_bool() };
    let active = VIEWS.with(|views| {
        let views = views.borrow();
        let active = navigation_surface_active(
            visible,
            browsing,
            views.get(&key).map_or(0, |entry| entry.depth),
            loading,
        );
        if let Some(entry) = views.get(&key) {
            let _ = entry.webview.set_visible(active);
        }
        active
    });
    set_active(hwnd, active);
    set_host_mode(hwnd, active);
    resize(hwnd);
    super::scene_compositor::sync_window(hwnd, visible);
}

fn build_navigation(hwnd: HWND, page_url: &str) -> anyhow::Result<WebView> {
    let key = hwnd.0 as isize;
    let wrapper = HwndWrapper(hwnd);
    let bounds = content_bounds(hwnd);
    let _init = crate::overlay::GLOBAL_WEBVIEW_MUTEX.lock().unwrap();
    CONTEXT.with(|context| {
        let mut context = context.borrow_mut();
        if context.is_none() {
            *context = Some(crate::overlay::webview_runtime::create_context(
                crate::overlay::webview_runtime::Profile::ResultNavigation,
            ));
        }
        WebViewBuilder::new_with_web_context(context.as_mut().unwrap())
            .with_bounds(bounds)
            .with_url(page_url)
            .with_transparent(true)
            .with_visible(false)
            .with_focused(false)
            .with_initialization_script(navigation_initialization_script())
            .with_on_page_load_handler(move |event, _| match event {
                PageLoadEvent::Started => begin_navigation_load(hwnd),
                PageLoadEvent::Finished => {
                    crate::debug_log::log_debug(&format!(
                        "[ResultNavigation] id={key} phase=page_loaded"
                    ));
                    finish_navigation_load(hwnd);
                }
            })
            .with_ipc_handler(move |request| handle_ipc(hwnd, request.body()))
            .build_as_child(&wrapper)
            .map_err(Into::into)
    })
}

fn handle_ipc(hwnd: HWND, body: &str) {
    match body {
        "navigation_surface_interaction" => focus(hwnd),
        "navigation_surface_ready" => {
            let trace_id = super::WINDOW_STATES
                .lock()
                .unwrap()
                .get(&(hwnd.0 as isize))
                .and_then(|state| state.latency_trace_id.clone());
            if let Some(trace_id) = trace_id {
                super::latency::mark(&trace_id, "interactive_surface_visible");
                VIEWS.with(|views| {
                    if let Some(entry) = views.borrow().get(&(hwnd.0 as isize)) {
                        super::scene_compositor::acceptance_capture::capture_for_trace(
                            &entry.webview,
                            trace_id,
                        );
                    }
                });
            }
        }
        _ => {
            let request = serde_json::from_str::<serde_json::Value>(body).ok();
            let request_type = request
                .as_ref()
                .and_then(|value| value.get("type"))
                .and_then(|value| value.as_str());
            if let Some("navigation_request") = request_type
                && let Some(url) = request
                    .as_ref()
                    .and_then(|value| value.get("url"))
                    .and_then(|value| value.as_str())
            {
                request_navigation(hwnd, url);
            }
        }
    }
}

pub(super) fn navigate_pending(hwnd: HWND) {
    let key = hwnd.0 as isize;
    let Some(url) = PENDING_NAVIGATION.lock().unwrap().remove(&key) else {
        return;
    };
    let existing = VIEWS.with(|views| {
        let mut views = views.borrow_mut();
        let Some(entry) = views.get_mut(&key) else {
            return false;
        };
        entry.urls.truncate(entry.depth);
        entry.urls.push(url.clone());
        entry.depth += 1;
        entry.loading = true;
        let _ = entry.webview.set_visible(false);
        let _ = entry.webview.load_url(&url);
        true
    });
    if !existing {
        match build_navigation(hwnd, &url) {
            Ok(webview) => VIEWS.with(|views| {
                views.borrow_mut().insert(
                    key,
                    RawView {
                        webview,
                        urls: vec![url],
                        depth: 1,
                        loading: true,
                    },
                );
            }),
            Err(error) => {
                crate::debug_log::log_debug(&format!(
                    "[ResultNavigation] id={key} phase=navigation_failed error={error}"
                ));
                return;
            }
        }
    }
    present_navigation(hwnd, true);
    if EARLY_FINISHED_LOADS.lock().unwrap().remove(&key) {
        finish_navigation_load(hwnd);
    }
}

pub(super) fn go_back(hwnd: HWND) -> bool {
    let key = hwnd.0 as isize;
    let loading = VIEWS.with(|views| {
        let mut views = views.borrow_mut();
        let entry = views.get_mut(&key)?;
        if entry.depth == 0 {
            return None;
        }
        entry.depth -= 1;
        if entry.depth > 0 {
            entry.loading = true;
            let _ = entry.webview.set_visible(false);
            let _ = entry.webview.load_url(&entry.urls[entry.depth - 1]);
            return Some(true);
        }
        entry.loading = false;
        let _ = entry.webview.set_visible(false);
        Some(false)
    });
    if let Some(loading) = loading {
        present_navigation(hwnd, loading);
        true
    } else {
        false
    }
}

pub(super) fn go_forward(hwnd: HWND) -> bool {
    let key = hwnd.0 as isize;
    let loading = VIEWS.with(|views| {
        let mut views = views.borrow_mut();
        let entry = views.get_mut(&key)?;
        if entry.depth >= entry.urls.len() {
            return None;
        }
        let url = entry.urls[entry.depth].clone();
        entry.depth += 1;
        entry.loading = true;
        let _ = entry.webview.set_visible(false);
        let _ = entry.webview.load_url(&url);
        Some(true)
    });
    if let Some(loading) = loading {
        present_navigation(hwnd, loading);
        true
    } else {
        false
    }
}

fn finish_navigation_load(hwnd: HWND) {
    let key = hwnd.0 as isize;
    let should_present = VIEWS.with(|views| {
        let mut views = views.borrow_mut();
        let Some(entry) = views.get_mut(&key) else {
            EARLY_FINISHED_LOADS.lock().unwrap().insert(key);
            return false;
        };
        if !entry.loading || entry.depth == 0 {
            return false;
        }
        entry.loading = false;
        true
    });
    if should_present {
        present_navigation(hwnd, false);
    }
}

fn begin_navigation_load(hwnd: HWND) {
    let should_present = VIEWS.with(|views| {
        let mut views = views.borrow_mut();
        let Some(entry) = views.get_mut(&(hwnd.0 as isize)) else {
            return false;
        };
        if entry.loading || entry.depth == 0 {
            return false;
        }
        entry.loading = true;
        let _ = entry.webview.set_visible(false);
        true
    });
    if should_present {
        present_navigation(hwnd, true);
    }
}

fn present_navigation(hwnd: HWND, loading: bool) {
    let key = hwnd.0 as isize;
    let (depth, max_depth) = VIEWS.with(|views| {
        let views = views.borrow();
        let entry = views.get(&key).expect("raw navigation view must exist");
        (entry.depth, entry.urls.len())
    });
    let browsing = depth > 0;
    {
        let mut states = super::WINDOW_STATES.lock().unwrap();
        let Some(state) = states.get_mut(&key) else {
            return;
        };
        state.navigation_depth = depth;
        state.max_navigation_depth = max_depth;
        state.is_browsing = browsing;
        state.is_navigation_loading = browsing && loading;
        if state.is_browsing {
            state.is_editing = false;
        }
    }
    let active = browsing && !loading;
    VIEWS.with(|views| {
        if let Some(entry) = views.borrow().get(&key) {
            let _ = entry.webview.set_visible(active);
        }
    });
    set_active(hwnd, active);
    set_host_mode(hwnd, active);
    resize(hwnd);
    super::scene_compositor::sync_window(hwnd, unsafe { IsWindowVisible(hwnd).as_bool() });
    super::scene_compositor::sync_controls(hwnd);
    if active {
        focus(hwnd);
    }
}

pub(super) fn focus(hwnd: HWND) {
    if !should_activate_foreground(super::scene_compositor::acceptance_offscreen()) {
        super::raise_window(hwnd);
        return;
    }
    super::scene_compositor::activation::activate_window(hwnd);
    VIEWS.with(|views| {
        if let Some(entry) = views.borrow().get(&(hwnd.0 as isize)) {
            let _ = entry.webview.focus();
        }
    });
    super::raise_window(hwnd);
}

fn should_activate_foreground(offscreen_acceptance: bool) -> bool {
    !offscreen_acceptance
}

fn navigation_surface_active(visible: bool, browsing: bool, depth: usize, loading: bool) -> bool {
    visible && browsing && depth > 0 && !loading
}

pub(super) fn raise_window(hwnd: HWND) {
    if !is_active(hwnd) {
        return;
    }
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }
}

pub(super) fn resize(hwnd: HWND) {
    let bounds = content_bounds(hwnd);
    VIEWS.with(|views| {
        if let Some(entry) = views.borrow().get(&(hwnd.0 as isize)) {
            let _ = entry.webview.set_bounds(bounds);
        }
    });
}

pub(super) fn destroy(hwnd: HWND) {
    remove_view(hwnd, false);
}

fn remove_view(hwnd: HWND, restore_host: bool) {
    let key = hwnd.0 as isize;
    PENDING_NAVIGATION.lock().unwrap().remove(&key);
    EARLY_FINISHED_LOADS.lock().unwrap().remove(&key);
    VIEWS.with(|views| {
        views.borrow_mut().remove(&key);
    });
    let removed = ACTIVE.lock().unwrap().remove(&key);
    if restore_host && removed {
        set_host_mode(hwnd, false);
        super::scene_compositor::sync_window(hwnd, unsafe { IsWindowVisible(hwnd).as_bool() });
    }
}

fn set_active(hwnd: HWND, active: bool) {
    let key = hwnd.0 as isize;
    let mut active_views = ACTIVE.lock().unwrap();
    if active {
        active_views.insert(key);
    } else {
        active_views.remove(&key);
    }
}

fn public_web_url(raw: &str) -> Option<String> {
    let parsed = url::Url::parse(raw.trim()).ok()?;
    matches!(parsed.scheme(), "http" | "https").then(|| parsed.into())
}

fn navigation_initialization_script() -> String {
    include_str!("navigation_runtime.js").to_string()
}

fn content_bounds(hwnd: HWND) -> Rect {
    let mut client = RECT::default();
    unsafe {
        let _ = GetClientRect(hwnd, &mut client);
    }
    Rect {
        position: wry::dpi::Position::Physical(wry::dpi::PhysicalPosition::new(0, 0)),
        size: wry::dpi::Size::Physical(wry::dpi::PhysicalSize::new(
            (client.right - client.left).max(1) as u32,
            (client.bottom - client.top).max(1) as u32,
        )),
    }
}

fn host_alpha(raw: bool) -> u8 {
    if raw { 255 } else { 1 }
}

fn set_host_mode(hwnd: HWND, raw: bool) {
    if !raw {
        super::scene_compositor::activation::restore_nonactivating_style(hwnd);
    }
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
        let style = if raw {
            style | WS_CLIPCHILDREN.0
        } else {
            style & !WS_CLIPCHILDREN.0
        };
        let _ = SetWindowLongPtrW(hwnd, GWL_STYLE, style as isize);
        let _ = SetWindowRgn(hwnd, None, true);
        let corner_preference = if raw { DWMWCP_ROUND } else { DWMWCP_DONOTROUND };
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            std::ptr::addr_of!(corner_preference).cast(),
            std::mem::size_of_val(&corner_preference) as u32,
        );
        let alpha = host_alpha(raw);
        let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), alpha, LWA_ALPHA);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        WM_NAVIGATE, WM_SYNC, content_bounds, host_alpha, navigation_initialization_script,
        navigation_surface_active, public_web_url, should_activate_foreground,
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
    fn native_page_visibility_is_bracketed_by_top_level_load_events() {
        let source = include_str!("raw_webview.rs");

        assert!(source.contains(".with_visible(false)"));
        assert!(source.contains("PageLoadEvent::Started => begin_navigation_load(hwnd)"));
        assert!(source.contains("PageLoadEvent::Finished =>"));
        assert!(source.contains("present_navigation(hwnd, false)"));
    }

    #[test]
    fn external_navigation_page_script_does_not_own_overlay_geometry() {
        let script = navigation_initialization_script();
        assert!(!script.contains("resize"));
        assert!(!script.contains("scrollbar"));
        assert!(!script.contains("anchor.target === '_blank'"));
    }
}
