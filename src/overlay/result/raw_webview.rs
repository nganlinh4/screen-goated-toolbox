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
use wry::{Rect, WebView, WebViewBuilder};

mod navigation_events;
mod navigation_history;
use navigation_events::{NavigationEvent, NavigationEvents};
use navigation_history::NavigationHistory;
use std::rc::Rc;

pub(super) const WM_SYNC: u32 = WM_APP + 241;
pub(super) const WM_NAVIGATE: u32 = WM_APP + 242;
pub(super) const WM_NAVIGATION_EVENT: u32 = WM_APP + 243;
static ACTIVE: LazyLock<Mutex<HashSet<isize>>> = LazyLock::new(|| Mutex::new(HashSet::new()));
static PENDING_NAVIGATION: LazyLock<Mutex<HashMap<isize, String>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

struct RawView {
    webview: WebView,
    history: NavigationHistory,
    events: Rc<RefCell<NavigationEvents>>,
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
            views.get(&key).map_or(0, |entry| entry.history.depth()),
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

fn build_navigation(hwnd: HWND) -> anyhow::Result<(WebView, Rc<RefCell<NavigationEvents>>)> {
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
        let webview = WebViewBuilder::new_with_web_context(context.as_mut().unwrap())
            .with_bounds(bounds)
            .with_transparent(true)
            .with_visible(false)
            .with_focused(false)
            .with_initialization_script(navigation_initialization_script())
            .with_new_window_req_handler(|_, _| wry::NewWindowResponse::Deny)
            .with_download_started_handler(|_, _| false)
            .build_as_child(&wrapper)
            .map_err(anyhow::Error::from)?;
        let events = navigation_events::attach(&webview, hwnd)?;
        Ok((webview, events))
    })
}

pub(super) fn navigate_pending(hwnd: HWND) {
    let key = hwnd.0 as isize;
    let Some(url) = PENDING_NAVIGATION.lock().unwrap().remove(&key) else {
        return;
    };
    let existing = VIEWS.with(|views| {
        let mut views = views.borrow_mut();
        let entry = views.get_mut(&key)?;
        entry.events.borrow_mut().invalidate();
        entry.events.borrow_mut().expect_url(&url);
        entry.history.open(url.clone());
        entry.loading = true;
        let _ = entry.webview.set_visible(false);
        Some(entry.webview.load_url(&url).is_ok())
    });
    if existing == Some(false) {
        fail_navigation_load(hwnd);
        return;
    }
    if existing.is_none() {
        match build_navigation(hwnd) {
            Ok((webview, events)) => {
                let started = VIEWS.with(|views| {
                    views.borrow_mut().insert(
                        key,
                        RawView {
                            webview,
                            history: NavigationHistory::new(url.clone()),
                            events,
                            loading: true,
                        },
                    );
                    if let Some(entry) = views.borrow().get(&key) {
                        entry.events.borrow_mut().expect_url(&url);
                        entry.webview.load_url(&url).is_ok()
                    } else {
                        false
                    }
                });
                if !started {
                    fail_navigation_load(hwnd);
                    return;
                }
            }
            Err(error) => {
                crate::debug_log::log_debug(&format!(
                    "[ResultNavigation] id={key} phase=navigation_failed error={error}"
                ));
                return;
            }
        }
    }
    present_navigation(hwnd, true);
}

pub(super) fn go_back(hwnd: HWND) -> bool {
    let key = hwnd.0 as isize;
    let result = VIEWS.with(|views| {
        let mut views = views.borrow_mut();
        let entry = views.get_mut(&key)?;
        entry.events.borrow_mut().invalidate();
        if let Some(url) = entry.history.back()? {
            entry.events.borrow_mut().expect_url(&url);
            entry.loading = true;
            let _ = entry.webview.set_visible(false);
            return Some((true, entry.webview.load_url(&url).is_ok()));
        }
        entry.loading = false;
        entry.events.borrow_mut().retire();
        let _ = entry.webview.set_visible(false);
        Some((false, true))
    });
    if let Some((loading, started)) = result {
        if !started {
            fail_navigation_load(hwnd);
            return true;
        }
        present_navigation(hwnd, loading);
        true
    } else {
        false
    }
}

pub(super) fn go_forward(hwnd: HWND) -> bool {
    let key = hwnd.0 as isize;
    let result = VIEWS.with(|views| {
        let mut views = views.borrow_mut();
        let entry = views.get_mut(&key)?;
        let url = entry.history.forward()?;
        entry.events.borrow_mut().invalidate();
        entry.events.borrow_mut().expect_url(&url);
        entry.loading = true;
        let _ = entry.webview.set_visible(false);
        Some(entry.webview.load_url(&url).is_ok())
    });
    if let Some(started) = result {
        if !started {
            fail_navigation_load(hwnd);
            return true;
        }
        present_navigation(hwnd, true);
        true
    } else {
        false
    }
}

pub(super) fn handle_navigation_event(hwnd: HWND) {
    let (event, same_document) = VIEWS.with(|views| {
        let views = views.borrow();
        views
            .get(&(hwnd.0 as isize))
            .map(|entry| entry.events.borrow_mut().take())
            .unwrap_or_default()
    });
    match event {
        Some(NavigationEvent::Started) => begin_navigation_load(hwnd),
        Some(NavigationEvent::Finished {
            success: true,
            url: Some(url),
        }) => {
            begin_navigation_load(hwnd);
            finish_navigation_load(hwnd, &url);
        }
        Some(NavigationEvent::Finished { .. }) => {
            begin_navigation_load(hwnd);
            fail_navigation_load(hwnd);
        }
        None => {}
    }
    if let Some(url) = same_document {
        let changed = VIEWS.with(|views| {
            let mut views = views.borrow_mut();
            let Some(entry) = views.get_mut(&(hwnd.0 as isize)) else {
                return false;
            };
            if entry.loading || entry.history.depth() == 0 {
                return false;
            }
            entry.history.finished(Some(url));
            true
        });
        if changed {
            present_navigation(hwnd, false);
        }
    }
}

fn fail_navigation_load(hwnd: HWND) {
    let changed = VIEWS.with(|views| {
        let mut views = views.borrow_mut();
        let Some(entry) = views.get_mut(&(hwnd.0 as isize)) else {
            return false;
        };
        if !entry.loading {
            return false;
        }
        entry.loading = false;
        entry.events.borrow_mut().retire();
        entry.history.retire_failed();
        true
    });
    if changed {
        present_navigation(hwnd, false);
    }
}

fn finish_navigation_load(hwnd: HWND, url: &str) {
    let key = hwnd.0 as isize;
    let should_present = VIEWS.with(|views| {
        let mut views = views.borrow_mut();
        let Some(entry) = views.get_mut(&key) else {
            return false;
        };
        if !entry.loading || entry.history.depth() == 0 {
            return false;
        }
        entry.history.finished(public_web_url(url));
        entry.loading = false;
        true
    });
    if should_present {
        present_navigation(hwnd, false);
        let trace_id = super::WINDOW_STATES
            .lock()
            .unwrap()
            .get(&key)
            .and_then(|state| state.latency_trace_id.clone());
        if let Some(trace_id) = trace_id {
            super::latency::mark(&trace_id, "interactive_surface_visible");
            VIEWS.with(|views| {
                if let Some(entry) = views.borrow().get(&key) {
                    super::scene_compositor::acceptance_capture::capture_for_trace(
                        &entry.webview,
                        trace_id,
                    );
                }
            });
        }
    }
}

fn begin_navigation_load(hwnd: HWND) {
    let should_present = VIEWS.with(|views| {
        let mut views = views.borrow_mut();
        let Some(entry) = views.get_mut(&(hwnd.0 as isize)) else {
            return false;
        };
        if entry.loading || entry.history.depth() == 0 {
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
        (entry.history.depth(), entry.history.max_depth())
    });
    let view = navigation_view(depth, loading);
    {
        let mut states = super::WINDOW_STATES.lock().unwrap();
        let Some(state) = states.get_mut(&key) else {
            return;
        };
        state.navigation_depth = depth;
        state.max_navigation_depth = max_depth;
        state.is_browsing = view.browsing;
        state.is_navigation_loading = view.loading;
        if state.is_browsing {
            state.refine_session.cancel_edit();
        }
    }
    let active = view.visible;
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

/// How a navigation history position is presented. A page stays hidden
/// while its top-level load runs, so a half-loaded remote page never shows.
#[derive(Debug, PartialEq, Eq)]
struct NavigationView {
    browsing: bool,
    loading: bool,
    visible: bool,
}

fn navigation_view(depth: usize, loading: bool) -> NavigationView {
    let browsing = depth > 0;
    NavigationView {
        browsing,
        loading: browsing && loading,
        visible: browsing && !loading,
    }
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
    if raw.len() > 8192 {
        return None;
    }
    let parsed = url::Url::parse(raw.trim()).ok()?;
    (matches!(parsed.scheme(), "http" | "https") && parsed.as_str().len() <= 8192)
        .then(|| parsed.into())
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
mod tests;
