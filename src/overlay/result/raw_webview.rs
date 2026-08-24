use super::markdown_view::conversion::render_for_compositor;
use crate::win_types::HwndWrapper;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::{LazyLock, Mutex};
use windows::Win32::Foundation::{COLORREF, HWND, RECT};
use windows::Win32::UI::WindowsAndMessaging::{
    GWL_STYLE, GetClientRect, GetWindowLongPtrW, HWND_TOPMOST, IsWindowVisible, LWA_ALPHA,
    PostMessageW, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetLayeredWindowAttributes,
    SetWindowLongPtrW, SetWindowPos, WM_APP, WS_CLIPCHILDREN,
};
use wry::{PageLoadEvent, Rect, WebContext, WebView, WebViewBuilder};

pub(super) const WM_SYNC: u32 = WM_APP + 241;
pub(super) const WM_NAVIGATE: u32 = WM_APP + 242;
static ACTIVE: LazyLock<Mutex<HashSet<isize>>> = LazyLock::new(|| Mutex::new(HashSet::new()));
static PENDING_NAVIGATION: LazyLock<Mutex<HashMap<isize, String>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

enum RawRoot {
    Document { document: String, url: String },
    Shared { signature: String },
}

struct RawView {
    webview: WebView,
    root: RawRoot,
    urls: Vec<String>,
    depth: usize,
}

thread_local! {
    static VIEWS: RefCell<HashMap<isize, RawView>> = RefCell::new(HashMap::new());
    static CONTEXT: RefCell<Option<WebContext>> = const { RefCell::new(None) };
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
    let snapshot = super::WINDOW_STATES.lock().unwrap().get(&key).map(|state| {
        (
            state.full_text.clone(),
            state.is_refining,
            state.preset_prompt.clone(),
            state.input_text.clone(),
            state.is_streaming_active,
            state.opacity_percent,
            state.is_browsing,
        )
    });
    let Some((text, refining, prompt, input, streaming, opacity, browsing)) = snapshot else {
        destroy(hwnd);
        return;
    };
    let document = if streaming {
        None
    } else {
        render_for_compositor(&text, refining, &prompt, &input).isolated_document
    };
    let visible = unsafe { IsWindowVisible(hwnd).as_bool() };
    let shared = VIEWS.with(|views| {
        views.borrow().get(&key).is_some_and(
            |entry| matches!(&entry.root, RawRoot::Shared { signature } if signature == &text),
        )
    });
    if document.is_none() && shared {
        let active = visible && browsing;
        VIEWS.with(|views| {
            if let Some(entry) = views.borrow().get(&key) {
                let _ = entry.webview.set_visible(active);
            }
        });
        set_active(hwnd, active);
        set_host_mode(hwnd, active, opacity);
        resize(hwnd);
        super::scene_compositor::sync_window(hwnd, visible);
        return;
    }
    let Some(document) = document.filter(|_| visible) else {
        remove_view(hwnd, true);
        return;
    };
    let unchanged = VIEWS.with(|views| {
        views.borrow().get(&key).is_some_and(|entry| {
            matches!(&entry.root, RawRoot::Document { document: current, .. } if current == &document)
        })
    });
    if unchanged {
        set_host_mode(hwnd, true, opacity);
        resize(hwnd);
        return;
    }
    remove_view(hwnd, false);
    match build_document(hwnd, &document) {
        Ok((webview, url)) => {
            VIEWS.with(|views| {
                views.borrow_mut().insert(
                    key,
                    RawView {
                        webview,
                        root: RawRoot::Document { document, url },
                        urls: Vec::new(),
                        depth: 0,
                    },
                );
            });
            set_active(hwnd, true);
            set_host_mode(hwnd, true, opacity);
            resize(hwnd);
            super::scene_compositor::sync_window(hwnd, true);
        }
        Err(error) => {
            crate::debug_log::log_debug(&format!(
                "[RawResult] id={key} phase=create_failed error={error}"
            ));
            set_host_mode(hwnd, false, opacity);
        }
    }
}

fn build_document(hwnd: HWND, document: &str) -> anyhow::Result<(WebView, String)> {
    let Some(page_url) =
        crate::overlay::html_components::font_manager::store_html_page(document.to_string())
    else {
        anyhow::bail!("internal page store unavailable");
    };
    Ok((build(hwnd, &page_url)?, page_url))
}

fn build(hwnd: HWND, page_url: &str) -> anyhow::Result<WebView> {
    let key = hwnd.0 as isize;
    let wrapper = HwndWrapper(hwnd);
    let bounds = content_bounds(hwnd);
    let _init = crate::overlay::GLOBAL_WEBVIEW_MUTEX.lock().unwrap();
    CONTEXT.with(|context| {
        let mut context = context.borrow_mut();
        if context.is_none() {
            *context = Some(WebContext::new(Some(
                crate::overlay::get_shared_webview_data_dir(Some("result-raw")),
            )));
        }
        WebViewBuilder::new_with_web_context(context.as_mut().unwrap())
            .with_bounds(bounds)
            .with_url(page_url)
            .with_transparent(true)
            .with_focused(false)
            .with_initialization_script(
                "addEventListener('load',()=>requestAnimationFrame(()=>requestAnimationFrame(()=>window.ipc.postMessage('raw_document_ready'))));document.addEventListener('pointerdown',()=>window.ipc.postMessage('raw_document_interaction'),true);document.addEventListener('click',event=>{const anchor=event.target?.closest?.('a[href]');if(!anchor||anchor.target==='_blank'||event.defaultPrevented||!/^https?:\\/\\//i.test(anchor.href))return;event.preventDefault();window.ipc.postMessage(JSON.stringify({type:'raw_navigation_request',url:anchor.href}));},true);",
            )
            .with_on_page_load_handler(move |event, _| {
                if matches!(event, PageLoadEvent::Finished) {
                    crate::debug_log::log_debug(&format!(
                        "[RawResult] id={key} phase=document_loaded"
                    ));
                }
            })
            .with_ipc_handler(move |request| handle_ipc(hwnd, request.body()))
            .build_as_child(&wrapper)
            .map_err(Into::into)
    })
}

fn handle_ipc(hwnd: HWND, body: &str) {
    match body {
        "raw_document_interaction" => focus(hwnd),
        "raw_document_ready" => {
            let trace_id = super::WINDOW_STATES
                .lock()
                .unwrap()
                .get(&(hwnd.0 as isize))
                .and_then(|state| state.latency_trace_id.clone());
            if let Some(trace_id) = trace_id {
                super::latency::mark(&trace_id, "interactive_document_alive");
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
            if request
                .as_ref()
                .and_then(|value| value.get("type"))
                .and_then(|value| value.as_str())
                == Some("raw_navigation_request")
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
        let _ = entry.webview.set_visible(true);
        let _ = entry.webview.load_url(&url);
        true
    });
    if !existing {
        let signature = super::WINDOW_STATES
            .lock()
            .unwrap()
            .get(&key)
            .map(|state| state.full_text.clone())
            .unwrap_or_default();
        match build(hwnd, &url) {
            Ok(webview) => VIEWS.with(|views| {
                views.borrow_mut().insert(
                    key,
                    RawView {
                        webview,
                        root: RawRoot::Shared { signature },
                        urls: vec![url],
                        depth: 1,
                    },
                );
            }),
            Err(error) => {
                crate::debug_log::log_debug(&format!(
                    "[RawResult] id={key} phase=navigation_failed error={error}"
                ));
                return;
            }
        }
    }
    present_navigation(hwnd);
}

pub(super) fn go_back(hwnd: HWND) -> bool {
    let key = hwnd.0 as isize;
    let handled = VIEWS.with(|views| {
        let mut views = views.borrow_mut();
        let Some(entry) = views.get_mut(&key) else {
            return false;
        };
        if entry.depth == 0 {
            return false;
        }
        entry.depth -= 1;
        if entry.depth > 0 {
            let _ = entry.webview.load_url(&entry.urls[entry.depth - 1]);
            return true;
        }
        match &entry.root {
            RawRoot::Document { url, .. } => {
                let _ = entry.webview.load_url(url);
            }
            RawRoot::Shared { .. } => {
                let _ = entry.webview.set_visible(false);
            }
        }
        true
    });
    if handled {
        present_navigation(hwnd);
    }
    handled
}

pub(super) fn go_forward(hwnd: HWND) -> bool {
    let key = hwnd.0 as isize;
    let handled = VIEWS.with(|views| {
        let mut views = views.borrow_mut();
        let Some(entry) = views.get_mut(&key) else {
            return false;
        };
        if entry.depth >= entry.urls.len() {
            return false;
        }
        let url = entry.urls[entry.depth].clone();
        entry.depth += 1;
        let _ = entry.webview.set_visible(true);
        let _ = entry.webview.load_url(&url);
        true
    });
    if handled {
        present_navigation(hwnd);
    }
    handled
}

fn present_navigation(hwnd: HWND) {
    let key = hwnd.0 as isize;
    let (depth, max_depth, active) = VIEWS.with(|views| {
        let views = views.borrow();
        let entry = views.get(&key).expect("raw navigation view must exist");
        let active = entry.depth > 0 || matches!(&entry.root, RawRoot::Document { .. });
        (entry.depth, entry.urls.len(), active)
    });
    let opacity = {
        let mut states = super::WINDOW_STATES.lock().unwrap();
        let Some(state) = states.get_mut(&key) else {
            return;
        };
        state.navigation_depth = depth;
        state.max_navigation_depth = max_depth;
        state.is_browsing = depth > 0;
        if state.is_browsing {
            state.is_editing = false;
        }
        state.opacity_percent
    };
    set_active(hwnd, active);
    set_host_mode(hwnd, active, opacity);
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
        set_host_mode(hwnd, false, 1);
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

fn content_bounds(hwnd: HWND) -> Rect {
    let mut client = RECT::default();
    unsafe {
        let _ = GetClientRect(hwnd, &mut client);
    }
    Rect {
        position: wry::dpi::Position::Physical(wry::dpi::PhysicalPosition::new(4, 2)),
        size: wry::dpi::Size::Physical(wry::dpi::PhysicalSize::new(
            (client.right - client.left - 8).max(1) as u32,
            (client.bottom - client.top - 4).max(1) as u32,
        )),
    }
}

fn set_host_mode(hwnd: HWND, raw: bool, opacity: u8) {
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
        let alpha = if raw {
            ((u16::from(opacity) * 255 / 100).max(1)) as u8
        } else {
            1
        };
        let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), alpha, LWA_ALPHA);
    }
}

#[cfg(test)]
mod tests {
    use super::{WM_NAVIGATE, WM_SYNC, public_web_url, should_activate_foreground};
    use windows::Win32::UI::WindowsAndMessaging::WM_APP;

    #[test]
    fn raw_document_sync_uses_a_private_window_message() {
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
}
