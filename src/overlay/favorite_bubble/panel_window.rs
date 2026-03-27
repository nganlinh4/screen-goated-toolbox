// --- PANEL WINDOW CREATION & WNDPROC ---
// Contains the Win32 window creation, WebView2 creation, and window procedure for the panel.

use super::html::generate_panel_html;
use super::panel::{close_panel, close_panel_internal, ensure_bubble_on_top};
use super::render::update_bubble_visual;
use super::state::*;
use super::utils::HwndWrapper;
use crate::APP;
use std::sync::atomic::Ordering;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::{
    DWMWA_WINDOW_CORNER_PREFERENCE, DwmExtendFrameIntoClientArea, DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Gdi::HBRUSH;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, GetClientRect, HTCAPTION, IDC_ARROW, LoadCursorW,
    RegisterClassW, SendMessageW, WM_ACTIVATE, WM_APP, WM_CLOSE, WM_KILLFOCUS, WM_NCCALCSIZE,
    WM_NCLBUTTONDOWN, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    WS_POPUP, WS_VISIBLE,
};
use windows::core::w;
use wry::{Rect, WebContext, WebViewBuilder};

pub(super) const WM_REFRESH_PANEL: u32 = WM_APP + 42;

pub(super) fn create_panel_window_internal(_bubble_hwnd: HWND) {
    unsafe {
        let instance = GetModuleHandleW(None).unwrap_or_default();
        let class_name = w!("SGTFavoritePanel");

        REGISTER_PANEL_CLASS.call_once(|| {
            let wc = WNDCLASSW {
                lpfnWndProc: Some(panel_wnd_proc),
                hInstance: instance.into(),
                lpszClassName: class_name,
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                hbrBackground: HBRUSH(std::ptr::null_mut()),
                ..Default::default()
            };
            RegisterClassW(&wc);
        });

        let panel_hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class_name,
            w!("FavPanel"),
            WS_POPUP | WS_VISIBLE,
            -4000,
            -4000,
            2000, // Dummy width (Large to avoid multi-column hit-test clipping)
            2000, // Dummy height (Large to avoid hit-test clipping)
            None,
            None,
            Some(instance.into()),
            None,
        )
        .unwrap_or_default();

        if !panel_hwnd.is_invalid() {
            PANEL_HWND.store(panel_hwnd.0 as isize, Ordering::SeqCst);

            // Windows 11 Rounded Corners - Disable native rounding
            let corner_pref = 1u32; // DWMWCP_DONOTROUND
            let _ = DwmSetWindowAttribute(
                panel_hwnd,
                DWMWA_WINDOW_CORNER_PREFERENCE,
                std::ptr::addr_of!(corner_pref) as *const _,
                std::mem::size_of_val(&corner_pref) as u32,
            );

            // Extend frame for transparency
            let margins = MARGINS {
                cxLeftWidth: -1,
                cxRightWidth: -1,
                cyTopHeight: -1,
                cyBottomHeight: -1,
            };
            let _ = DwmExtendFrameIntoClientArea(panel_hwnd, &margins);

            // NOTE: WebView2 creation is deferred to show_panel()
        }
    }
}

pub(super) fn create_panel_webview(panel_hwnd: HWND) {
    crate::log_info!("[BubblePanel] Creating WebView for HWND: {:?}", panel_hwnd);
    let mut rect = RECT::default();
    unsafe {
        let _ = GetClientRect(panel_hwnd, &mut rect);
    }

    let html = if let Ok(app) = APP.lock() {
        let is_dark = match app.config.theme_mode {
            crate::config::ThemeMode::Dark => true,
            crate::config::ThemeMode::Light => false,
            crate::config::ThemeMode::System => crate::gui::utils::is_system_in_dark_mode(),
        };
        // Update static state to match initial generation
        LAST_THEME_IS_DARK.store(is_dark, Ordering::SeqCst);
        generate_panel_html(
            &app.config.presets,
            &app.config.ui_language,
            is_dark,
            app.config.favorites_keep_open,
        )
    } else {
        String::new()
    };

    let wrapper = HwndWrapper(panel_hwnd);

    PANEL_WEB_CONTEXT.with(|ctx| {
        if ctx.borrow().is_none() {
            let shared_data_dir = crate::overlay::get_shared_webview_data_dir(Some("common"));
            *ctx.borrow_mut() = Some(WebContext::new(Some(shared_data_dir)));
        }
    });

    let result = {
        // LOCK SCOPE: Serialized build to prevent resource contention
        let _init_lock = crate::overlay::GLOBAL_WEBVIEW_MUTEX.lock().unwrap();
        crate::log_info!(
            "[BubblePanel] Acquired init lock. Building for HWND: {:?}...",
            panel_hwnd
        );

        let build_res = PANEL_WEB_CONTEXT.with(|ctx| {
            let mut ctx_ref = ctx.borrow_mut();
            let builder = if let Some(web_ctx) = ctx_ref.as_mut() {
                WebViewBuilder::new_with_web_context(web_ctx)
            } else {
                WebViewBuilder::new()
            };
            let builder = crate::overlay::html_components::font_manager::configure_webview(builder);

            // Store HTML in font server and get URL for same-origin font loading
            let page_url =
                crate::overlay::html_components::font_manager::store_html_page(html.clone())
                    .unwrap_or_else(|| format!("data:text/html,{}", urlencoding::encode(&html)));

            builder
                .with_bounds(Rect {
                    position: wry::dpi::Position::Physical(wry::dpi::PhysicalPosition::new(0, 0)),
                    size: wry::dpi::Size::Physical(wry::dpi::PhysicalSize::new(
                        (rect.right - rect.left) as u32,
                        (rect.bottom - rect.top) as u32,
                    )),
                })
                .with_url(&page_url)
                .with_transparent(true)
                .with_ipc_handler(move |msg: wry::http::Request<String>| {
                    handle_ipc_message(msg.body(), panel_hwnd);
                })
                .with_background_color((0, 0, 0, 0))
                .build(&wrapper)
        });
        crate::log_info!(
            "[BubblePanel] Build finished. Status: {}",
            if build_res.is_ok() { "OK" } else { "ERR" }
        );
        build_res
    };

    if let Ok(webview) = result {
        crate::log_info!("[BubblePanel] WebView success for HWND: {:?}", panel_hwnd);
        PANEL_WEBVIEW.with(|wv| {
            *wv.borrow_mut() = Some(webview);
        });
    } else if let Err(e) = result {
        crate::log_info!(
            "[BubblePanel] WebView FAILED for HWND: {:?}, Error: {:?}",
            panel_hwnd,
            e
        );
    }
}

fn handle_ipc_message(body: &str, panel_hwnd: HWND) {
    if body == "drag" {
        unsafe {
            use windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
            let _ = ReleaseCapture();
            SendMessageW(
                panel_hwnd,
                WM_NCLBUTTONDOWN,
                Some(WPARAM(HTCAPTION as usize)),
                Some(LPARAM(0)),
            );
        }
    } else if body == "close" {
        close_panel();
    } else if body == "close_now" {
        close_panel_internal();
    } else if body == "focus_bubble" {
        // Re-assert bubble Z-order on any click interaction
        ensure_bubble_on_top();
    } else if let Some(idx) = body.strip_prefix("trigger:") {
        if let Ok(idx) = idx.parse::<usize>() {
            IS_EXPANDED.store(false, Ordering::SeqCst);
            super::panel_actions::trigger_preset(idx);
        }
    } else if let Some(idx) = body.strip_prefix("trigger_only:") {
        // Keep Open mode: trigger preset without closing panel
        if let Ok(idx) = idx.parse::<usize>() {
            super::panel_actions::trigger_preset(idx);
            ensure_bubble_on_top();
        }
    } else if let Some(idx) = body.strip_prefix("trigger_continuous:") {
        if let Ok(idx) = idx.parse::<usize>() {
            IS_EXPANDED.store(false, Ordering::SeqCst);
            super::panel_actions::activate_continuous_from_panel(idx);
        }
    } else if let Some(idx) = body.strip_prefix("trigger_continuous_only:") {
        if let Ok(idx) = idx.parse::<usize>() {
            super::panel_actions::activate_continuous_from_panel(idx);
            ensure_bubble_on_top();
        }
    } else if let Some(val) = body.strip_prefix("set_keep_open:") {
        if let Ok(val) = val.parse::<u32>() {
            if let Ok(mut app) = APP.lock() {
                app.config.favorites_keep_open = val == 1;
                crate::config::save_config(&app.config);
            }
            ensure_bubble_on_top();
        }
    } else if let Some(h) = body.strip_prefix("resize:") {
        if let Ok(h) = h.parse::<i32>() {
            super::panel_actions::resize_panel_height(h);
        }
    } else if body == "increase_size" {
        if let Ok(mut app) = APP.lock() {
            let new_size = (app.config.favorite_bubble_size + 4).min(56);
            app.config.favorite_bubble_size = new_size;
            crate::config::save_config(&app.config);
            BUBBLE_SIZE.store(new_size as i32, Ordering::SeqCst);
        }
        super::panel::update_favorites_panel();
    } else if body == "decrease_size" {
        if let Ok(mut app) = APP.lock() {
            let new_size = (app.config.favorite_bubble_size.saturating_sub(4)).max(16);
            app.config.favorite_bubble_size = new_size;
            crate::config::save_config(&app.config);
            BUBBLE_SIZE.store(new_size as i32, Ordering::SeqCst);
        }
        super::panel::update_favorites_panel();
    }
}

pub(super) unsafe extern "system" fn panel_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        match msg {
            WM_CLOSE => {
                close_panel();
                LRESULT(0)
            }
            WM_KILLFOCUS => LRESULT(0),
            WM_ACTIVATE => {
                if wparam.0 == 0 {
                    // Window deactivated logic (optional)
                }
                LRESULT(0)
            }
            WM_REFRESH_PANEL => {
                let bubble_hwnd = HWND(BUBBLE_HWND.load(Ordering::SeqCst) as *mut std::ffi::c_void);

                if let Ok(app) = APP.lock() {
                    let is_dark = match app.config.theme_mode {
                        crate::config::ThemeMode::Dark => true,
                        crate::config::ThemeMode::Light => false,
                        crate::config::ThemeMode::System => {
                            crate::gui::utils::is_system_in_dark_mode()
                        }
                    };

                    // Set expanded to true so it moves with bubble
                    IS_EXPANDED.store(true, Ordering::SeqCst);

                    super::panel::refresh_panel_layout_and_content(
                        bubble_hwnd,
                        hwnd,
                        &app.config.presets,
                        &app.config.ui_language,
                        is_dark,
                    );
                }
                // Lock released here

                // Correctly call update_bubble_visual outside the lock
                update_bubble_visual(bubble_hwnd);

                LRESULT(0)
            }
            WM_NCCALCSIZE => {
                if wparam.0 != 0 {
                    LRESULT(0)
                } else {
                    DefWindowProcW(hwnd, msg, wparam, lparam)
                }
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}
