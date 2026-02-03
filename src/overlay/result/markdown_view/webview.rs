//! WebView creation and management for markdown view

use raw_window_handle::{
    HandleError, HasWindowHandle, RawWindowHandle, Win32WindowHandle, WindowHandle,
};
use std::num::NonZeroIsize;
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use wry::{Rect, WebViewBuilder};

use super::conversion::markdown_to_html;
use super::ipc::handle_markdown_ipc;
use super::navigation::update_markdown_content_ex;
use super::{SHARED_WEB_CONTEXT, SKIP_NEXT_NAVIGATION, WEBVIEWS, WEBVIEW_STATES};

/// Wrapper for HWND to implement HasWindowHandle
struct HwndWrapper(HWND);

impl HasWindowHandle for HwndWrapper {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let hwnd = self.0 .0 as isize;
        if let Some(non_zero) = NonZeroIsize::new(hwnd) {
            let mut handle = Win32WindowHandle::new(non_zero);
            // hinstance is optional, can be null
            handle.hinstance = None;
            let raw = RawWindowHandle::Win32(handle);
            // Safety: the handle is valid for the lifetime of HwndWrapper
            Ok(unsafe { WindowHandle::borrow_raw(raw) })
        } else {
            Err(HandleError::Unavailable)
        }
    }
}

/// Create a WebView child window for markdown rendering
/// Must be called from the main thread!
pub fn create_markdown_webview(parent_hwnd: HWND, markdown_text: &str, is_hovered: bool) -> bool {
    let hwnd_key = parent_hwnd.0 as isize;
    let (is_refining, preset_prompt, input_text) = {
        let states = crate::overlay::result::state::WINDOW_STATES.lock().unwrap();
        if let Some(state) = states.get(&hwnd_key) {
            (
                state.is_refining,
                state.preset_prompt.clone(),
                state.input_text.clone(),
            )
        } else {
            (false, String::new(), String::new())
        }
    };
    create_markdown_webview_ex(
        parent_hwnd,
        markdown_text,
        is_hovered,
        is_refining,
        &preset_prompt,
        &input_text,
    )
}

/// Create a WebView child window for markdown rendering (Internal version, call without lock if possible)
pub fn create_markdown_webview_ex(
    parent_hwnd: HWND,
    markdown_text: &str,
    _is_hovered: bool,
    is_refining: bool,
    preset_prompt: &str,
    input_text: &str,
) -> bool {
    let hwnd_key = parent_hwnd.0 as isize;

    // Check if we already have a webview
    let exists = WEBVIEWS.with(|webviews| webviews.borrow().contains_key(&hwnd_key));

    if exists {
        return update_markdown_content_ex(
            parent_hwnd,
            markdown_text,
            is_refining,
            preset_prompt,
            input_text,
        );
    }

    // Get parent window rect
    let mut rect = RECT::default();
    unsafe {
        let _ = GetClientRect(parent_hwnd, &mut rect);
    }
    crate::log_info!(
        "[Markdown] Creating WebView for Parent HWND: {:?}",
        parent_hwnd
    );

    let html_content = markdown_to_html(markdown_text, is_refining, preset_prompt, input_text);

    let wrapper = HwndWrapper(parent_hwnd);

    // Edge margins: 4px left/right for resize handles, 2px top/bottom
    let margin_x = 4.0;
    let margin_y = 2.0;
    let button_area_height = margin_y;
    let content_width = ((rect.right - rect.left) as f64 - margin_x * 2.0).max(50.0);
    let content_height = ((rect.bottom - rect.top) as f64 - margin_y - button_area_height).max(0.0);

    let hwnd_key_for_nav = hwnd_key;

    let full_html = html_content;

    // Use store_html_page with safe, minimal retry (max 100ms total block)
    let mut page_url = String::new();
    for _ in 0..10 {
        if let Some(url) =
            crate::overlay::html_components::font_manager::store_html_page(full_html.clone())
        {
            page_url = url;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    if page_url.is_empty() {
        crate::log_info!("[Markdown] FAILED to store markdown page in font server!");
        let error_html = "<html><body style='color:white'>Error: Could not connect to internal font server.</body></html>";
        if let Some(url) =
            crate::overlay::html_components::font_manager::store_html_page(error_html.to_string())
        {
            page_url = url;
        } else {
            page_url = format!("data:text/html,{}", urlencoding::encode(error_html));
        }
    }

    // Use SHARED_WEB_CONTEXT instead of creating a new one every time to keep RAM at 80MB
    let result = {
        // LOCK SCOPE: Serialized build to prevent resource contention
        let _init_lock = crate::overlay::GLOBAL_WEBVIEW_MUTEX.lock().unwrap();
        crate::log_info!(
            "[Markdown] Acquired init lock. Building for HWND: {:?}...",
            parent_hwnd
        );

        let build_res = SHARED_WEB_CONTEXT.with(|ctx| {
            let mut ctx_ref = ctx.borrow_mut();

            // Initialization check
            if ctx_ref.is_none() {
                let shared_data_dir = crate::overlay::get_shared_webview_data_dir(Some("common"));
                *ctx_ref = Some(wry::WebContext::new(Some(shared_data_dir)));
            }

            let mut builder = WebViewBuilder::new_with_web_context(ctx_ref.as_mut().unwrap())
                .with_bounds(Rect {
                    position: wry::dpi::Position::Physical(wry::dpi::PhysicalPosition::new(
                        margin_x as i32,
                        margin_y as i32,
                    )),
                    size: wry::dpi::Size::Physical(wry::dpi::PhysicalSize::new(
                        content_width as u32,
                        content_height as u32,
                    )),
                })
                .with_url(&page_url)
                .with_transparent(true);

            builder = builder.with_navigation_handler(move |url: String| {
                handle_navigation(hwnd_key_for_nav, &url)
            });

            builder = builder.with_ipc_handler(move |msg: wry::http::Request<String>| {
                handle_ipc(parent_hwnd, msg.body());
            });

            crate::overlay::html_components::font_manager::configure_webview(builder)
                .build_as_child(&wrapper)
        });

        crate::log_info!(
            "[Markdown] Build finished. Releasing lock. Status: {}",
            if build_res.is_ok() { "OK" } else { "ERR" }
        );
        build_res
    };

    match result {
        Ok(webview) => {
            crate::log_info!(
                "[Markdown] WebView success for Parent HWND: {:?}",
                parent_hwnd
            );
            WEBVIEWS.with(|webviews| {
                webviews.borrow_mut().insert(hwnd_key, webview);
            });

            let mut states = WEBVIEW_STATES.lock().unwrap();
            states.insert(hwnd_key, true);
            true
        }
        Err(e) => {
            crate::log_info!(
                "[Markdown] WebView FAILED for Parent HWND: {:?}, Error: {:?}",
                parent_hwnd,
                e
            );
            // WebView creation failed - warmup may not have completed
            false
        }
    }
}

/// Handle navigation events from WebView
fn handle_navigation(hwnd_key: isize, url: &str) -> bool {
    // Check if we should skip this navigation (triggered by history.back())
    let should_skip = {
        let mut skip_map = SKIP_NEXT_NAVIGATION.lock().unwrap();
        if skip_map.get(&hwnd_key).copied().unwrap_or(false) {
            skip_map.insert(hwnd_key, false);
            true
        } else {
            false
        }
    };

    if should_skip {
        // This navigation was from history.back(), don't increment depth
        return true;
    }

    // Detect when user navigates to an external URL (clicked a link)
    let is_internal = url.contains("wry.localhost")
        || url.contains("localhost")
        || url.contains("127.0.0.1")
        || url.starts_with("data:")
        || url.starts_with("about:");
    let is_external = (url.starts_with("http://") || url.starts_with("https://")) && !is_internal;

    if is_external {
        // Update browsing state and increment depth counter
        if let Ok(mut states) = crate::overlay::result::state::WINDOW_STATES.lock() {
            if let Some(state) = states.get_mut(&hwnd_key) {
                state.is_browsing = true;
                state.navigation_depth += 1;
                state.max_navigation_depth = state.navigation_depth;

                if state.is_editing {
                    state.is_editing = false;
                }
            }
        }
        crate::overlay::result::button_canvas::update_window_position(HWND(
            hwnd_key as *mut std::ffi::c_void,
        ));
    } else if is_internal {
        // If we hit an internal URL, we are likely back at the start (or initial load)
        if let Ok(mut states) = crate::overlay::result::state::WINDOW_STATES.lock() {
            if let Some(state) = states.get_mut(&hwnd_key) {
                if state.is_browsing {
                    state.is_browsing = false;
                    state.navigation_depth = 0;
                    state.max_navigation_depth = 0;
                    unsafe {
                        let _ = windows::Win32::Graphics::Gdi::InvalidateRect(
                            Some(HWND(hwnd_key as *mut std::ffi::c_void)),
                            None,
                            false,
                        );
                    }
                    crate::overlay::result::button_canvas::update_window_position(HWND(
                        hwnd_key as *mut std::ffi::c_void,
                    ));
                }
            }
        }
    }

    // Allow all navigation
    true
}

/// Handle IPC messages from WebView
fn handle_ipc(parent_hwnd: HWND, body: &str) {
    // Root IPC handler (general markdown actions)
    handle_markdown_ipc(parent_hwnd, body);

    if body.starts_with("opacity:") {
        if let Ok(opacity_percent) = body["opacity:".len()..].parse::<f32>() {
            let alpha = ((opacity_percent / 100.0) * 255.0) as u8;
            unsafe {
                use windows::Win32::Foundation::COLORREF;
                use windows::Win32::UI::WindowsAndMessaging::{LWA_ALPHA, SetLayeredWindowAttributes};
                let _ = SetLayeredWindowAttributes(parent_hwnd, COLORREF(0), alpha, LWA_ALPHA);
            }
        }
    }
}

/// Resize the WebView to match parent window
pub fn resize_markdown_webview(parent_hwnd: HWND, _is_hovered: bool) {
    let hwnd_key = parent_hwnd.0 as isize;

    let top_offset = 2.0; // 2px edge margin

    unsafe {
        let mut rect = RECT::default();
        let _ = GetClientRect(parent_hwnd, &mut rect);

        let margin_x = 4.0;
        let margin_y = 2.0;
        let button_area_height = margin_y;

        let content_width = ((rect.right - rect.left) as f64 - margin_x * 2.0).max(50.0);
        let content_height =
            ((rect.bottom - rect.top) as f64 - top_offset - button_area_height).max(0.0);

        WEBVIEWS.with(|webviews| {
            if let Some(webview) = webviews.borrow().get(&hwnd_key) {
                let _ = webview.set_bounds(Rect {
                    position: wry::dpi::Position::Physical(wry::dpi::PhysicalPosition::new(
                        margin_x as i32,
                        top_offset as i32,
                    )),
                    size: wry::dpi::Size::Physical(wry::dpi::PhysicalSize::new(
                        content_width as u32,
                        content_height as u32,
                    )),
                });
            }
        });
    }
}

/// Hide the WebView (toggle back to plain text)
pub fn hide_markdown_webview(parent_hwnd: HWND) {
    let hwnd_key = parent_hwnd.0 as isize;

    WEBVIEWS.with(|webviews| {
        if let Some(webview) = webviews.borrow().get(&hwnd_key) {
            let _ = webview.set_visible(false);
        }
    });
}

/// Show the WebView (toggle to markdown mode)
pub fn show_markdown_webview(parent_hwnd: HWND) {
    let hwnd_key = parent_hwnd.0 as isize;

    WEBVIEWS.with(|webviews| {
        if let Some(webview) = webviews.borrow().get(&hwnd_key) {
            let _ = webview.set_visible(true);
        }
    });
}

/// Destroy the WebView when window closes
pub fn destroy_markdown_webview(parent_hwnd: HWND) {
    let hwnd_key = parent_hwnd.0 as isize;

    WEBVIEWS.with(|webviews| {
        webviews.borrow_mut().remove(&hwnd_key);
    });

    let mut states = WEBVIEW_STATES.lock().unwrap();
    states.remove(&hwnd_key);
}

/// Check if markdown webview exists for this window
pub fn has_markdown_webview(parent_hwnd: HWND) -> bool {
    let hwnd_key = parent_hwnd.0 as isize;
    let states = WEBVIEW_STATES.lock().unwrap();
    states.get(&hwnd_key).copied().unwrap_or(false)
}
