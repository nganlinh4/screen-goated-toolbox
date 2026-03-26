// Window creation, WebView initialization, message loop, and window procedure

use std::sync::atomic::Ordering;

use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::{
    DWMWA_BORDER_COLOR, DWMWA_COLOR_NONE, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND,
    DwmExtendFrameIntoClientArea, DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::w;
use wry::{Rect, WebContext, WebViewBuilder};

use crate::APP;

use super::html::generate_popup_update_script;
use super::render::generate_popup_html;
use super::{
    get_scaled_dimension, hide_tray_popup, popup_window_dimensions, set_popup_bounds,
    HwndWrapper, IGNORE_FOCUS_LOSS_UNTIL, IS_WARMED_UP, IS_WARMING_UP, POPUP_HWND,
    POPUP_WEBVIEW, POPUP_WEB_CONTEXT, REGISTER_POPUP_CLASS, WARMUP_START_TIME,
    WEBVIEW_INIT_FAILED, WM_APP_SHOW, BASE_POPUP_HEIGHT, BASE_POPUP_WIDTH,
    POPUP_SURFACE_INSET,
};

/// Creates the popup window and runs its message loop forever.
/// This is called once during warmup - the window is kept alive hidden for reuse.
pub(super) fn create_popup_window() {
    unsafe {
        // Initialize COM for the thread (Critical for WebView2/Wry)
        let coinit = windows::Win32::System::Com::CoInitialize(None);
        crate::log_info!("[TrayPopup] Loop Start - CoInit: {:?}", coinit);

        let instance = GetModuleHandleW(None).unwrap_or_default();
        let class_name = w!("SGTTrayPopup");

        REGISTER_POPUP_CLASS.call_once(|| {
            let wc = WNDCLASSW {
                lpfnWndProc: Some(popup_wnd_proc),
                hInstance: instance.into(),
                lpszClassName: class_name,
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                hbrBackground: HBRUSH(std::ptr::null_mut()),
                ..Default::default()
            };
            RegisterClassW(&wc);
        });
        crate::log_info!("[TrayPopup] Class Registered");

        // Pre-size the transparent window for the optional restore flyout.
        let (popup_width, popup_height) = popup_window_dimensions();

        // Create hidden off-screen (will be repositioned when shown)
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED,
            class_name,
            w!("TrayPopup"),
            WS_POPUP,
            -3000,
            -3000,
            popup_width,
            popup_height,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .unwrap_or_default();

        crate::log_info!("[TrayPopup] Window created with HWND: {:?}", hwnd);

        if hwnd.is_invalid() {
            return;
        }

        POPUP_HWND.store(hwnd.0 as isize, Ordering::SeqCst);

        // Make transparent initially (invisible)
        let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 0, LWA_ALPHA);

        // Disable native rounding/borders; CSS handles the visible card corners.
        let corner_pref = DWMWCP_DONOTROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            std::ptr::addr_of!(corner_pref) as *const _,
            std::mem::size_of_val(&corner_pref) as u32,
        );
        let border_color = DWMWA_COLOR_NONE;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_BORDER_COLOR,
            std::ptr::addr_of!(border_color) as *const _,
            std::mem::size_of_val(&border_color) as u32,
        );
        let margins = MARGINS {
            cxLeftWidth: -1,
            cxRightWidth: -1,
            cyTopHeight: -1,
            cyBottomHeight: -1,
        };
        let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);

        // Create WebView using shared context for RAM efficiency
        let wrapper = HwndWrapper(hwnd);
        let html = generate_popup_html();

        // Initialize shared WebContext if needed (uses same data dir as other modules)
        POPUP_WEB_CONTEXT.with(|ctx| {
            if ctx.borrow().is_none() {
                // Consolidate all minor overlays to 'common' to share one browser process and keep RAM at ~80MB
                let shared_data_dir = crate::overlay::get_shared_webview_data_dir(Some("common"));
                *ctx.borrow_mut() = Some(WebContext::new(Some(shared_data_dir)));
            }
        });

        crate::log_info!("[TrayPopup] Starting WebView initialization...");

        let mut final_webview: Option<wry::WebView> = None;

        // Stagger startup to avoid collision
        std::thread::sleep(std::time::Duration::from_millis(250));

        for attempt in 1..=3 {
            let res = {
                // LOCK SCOPE: Only one WebView builds at a time to prevent "Not enough quota"
                let _init_lock = crate::overlay::GLOBAL_WEBVIEW_MUTEX.lock().unwrap();
                crate::log_info!(
                    "[TrayPopup] (Attempt {}) Acquired init lock. Building...",
                    attempt
                );

                POPUP_WEB_CONTEXT.with(|ctx| {
                    let mut ctx_ref = ctx.borrow_mut();
                    let builder = if let Some(web_ctx) = ctx_ref.as_mut() {
                        WebViewBuilder::new_with_web_context(web_ctx)
                    } else {
                        WebViewBuilder::new()
                    };
                    let builder = crate::overlay::html_components::font_manager::configure_webview(builder);

                    // Store HTML in font server and get URL for same-origin font loading
                    let page_url = crate::overlay::html_components::font_manager::store_html_page(html.clone())
                        .unwrap_or_else(|| format!("data:text/html,{}", urlencoding::encode(&html)));

                    builder
                        .with_bounds(Rect {
                            position: wry::dpi::Position::Logical(wry::dpi::LogicalPosition::new(0.0, 0.0)),
                            size: wry::dpi::Size::Physical(wry::dpi::PhysicalSize::new(
                                popup_width as u32,
                                popup_height as u32,
                            )),
                        })
                        .with_transparent(true)
                        .with_background_color((0, 0, 0, 0))
                        .with_url(&page_url)
                        .with_ipc_handler(move |msg: wry::http::Request<String>| {
                            handle_ipc_message(msg.body());
                        })
                        .build(&wrapper)
                })
            };

            crate::log_info!(
                "[TrayPopup] (Attempt {}) Release lock. Result: {}",
                attempt,
                if res.is_ok() { "OK" } else { "ERR" }
            );

            match res {
                Ok(wv) => {
                    final_webview = Some(wv);
                    break;
                }
                Err(e) => {
                    crate::log_info!(
                        "[TrayPopup] WebView init attempt {} failed: {:?}",
                        attempt,
                        e
                    );
                    std::thread::sleep(std::time::Duration::from_millis(2000));
                }
            }
        }

        if let Some(wv) = final_webview {
            crate::log_info!("[TrayPopup] WebView initialization SUCCESSFUL");
            POPUP_WEBVIEW.with(|cell| {
                *cell.borrow_mut() = Some(wv);
            });

            // Mark as warmed up - ready for instant display
            IS_WARMED_UP.store(true, Ordering::SeqCst);
            IS_WARMING_UP.store(false, Ordering::SeqCst); // Done warming up
            WARMUP_START_TIME.store(0, Ordering::SeqCst);

            // Message loop runs forever to keep window alive
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).into() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        } else {
            crate::log_info!("[TrayPopup] FAILED to initialize WebView after 3 attempts.");
            WEBVIEW_INIT_FAILED.store(true, Ordering::SeqCst);
        }

        // Clean up on thread exit
        IS_WARMED_UP.store(false, Ordering::SeqCst);
        IS_WARMING_UP.store(false, Ordering::SeqCst);
        POPUP_HWND.store(0, Ordering::SeqCst);
        WARMUP_START_TIME.store(0, Ordering::SeqCst);
        POPUP_WEBVIEW.with(|cell| {
            *cell.borrow_mut() = None;
        });

        windows::Win32::System::Com::CoUninitialize();
    }
}

/// Handle IPC messages from the WebView
fn handle_ipc_message(body: &str) {
    match body {
        "settings" => {
            // Hide popup and restore main window
            hide_tray_popup();
            crate::gui::signal_restore_window();
        }
        "bubble" => {
            // Toggle bubble state
            let new_state = if let Ok(mut app) = APP.lock() {
                app.config.show_favorite_bubble = !app.config.show_favorite_bubble;
                let enabled = app.config.show_favorite_bubble;
                crate::config::save_config(&app.config);

                if enabled {
                    crate::overlay::favorite_bubble::show_favorite_bubble();
                    std::thread::spawn(|| {
                        std::thread::sleep(std::time::Duration::from_millis(150));
                        crate::overlay::favorite_bubble::trigger_blink_animation();
                    });
                } else {
                    crate::overlay::favorite_bubble::hide_favorite_bubble();
                }
                enabled
            } else {
                false
            };

            // Update checkmark in popup via JavaScript (keep popup open)
            POPUP_WEBVIEW.with(|cell| {
                if let Some(webview) = cell.borrow().as_ref() {
                    let js = format!(
                        "document.getElementById('bubble-check-container').innerHTML = '{}';",
                        if new_state {
                            r#"<svg class="check-icon" viewBox="0 0 16 16" fill="currentColor"><path d="M13.86 3.66a.75.75 0 0 1 0 1.06l-7.25 7.25a.75.75 0 0 1-1.06 0L2.6 9.03a.75.75 0 1 1 1.06-1.06l2.42 2.42 6.72-6.72a.75.75 0 0 1 1.06 0z"/></svg>"#
                        } else { "" }
                    );
                    let _ = webview.evaluate_script(&js);
                }
            });
        }
        "stop_tts" => {
            // Stop all TTS playback and clear queues
            crate::api::tts::TTS_MANAGER.stop();
            // Hide popup after action
            hide_tray_popup();
        }
        "restore_overlay" => {
            hide_tray_popup();
            std::thread::spawn(|| {
                std::thread::sleep(std::time::Duration::from_millis(60));
                let _ = crate::overlay::result::restore_last_closed();
            });
        }
        body if body.starts_with("restore_recent:") => {
            let batch_count = body
                .split_once(':')
                .and_then(|(_, value)| value.parse::<usize>().ok())
                .unwrap_or(1);
            hide_tray_popup();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(60));
                let _ = crate::overlay::result::restore_recent(batch_count);
            });
        }
        "quit" => {
            // Hide popup first, then exit
            hide_tray_popup();
            std::thread::spawn(|| {
                std::thread::sleep(std::time::Duration::from_millis(50));
                std::process::exit(0);
            });
        }
        "close" => {
            hide_tray_popup();
        }
        _ => {}
    }
}

pub(super) unsafe extern "system" fn popup_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        match msg {
            WM_APP_SHOW => {
                // Reposition window to cursor and show
                let (popup_width, popup_height) = popup_window_dimensions();
                let popup_inset = get_scaled_dimension(POPUP_SURFACE_INSET);
                let main_width = get_scaled_dimension(BASE_POPUP_WIDTH);
                let main_height = get_scaled_dimension(BASE_POPUP_HEIGHT);

                let mut pt = POINT::default();
                let _ = GetCursorPos(&mut pt);
                let screen_w = GetSystemMetrics(SM_CXSCREEN);
                let screen_h = GetSystemMetrics(SM_CYSCREEN);

                let main_x = (pt.x - main_width / 2)
                    .max(0)
                    .min((screen_w - main_width).max(0));
                let popup_x = (main_x - popup_inset)
                    .max(0)
                    .min((screen_w - popup_width).max(0));
                let popup_y = (pt.y - main_height - popup_inset - 10)
                    .max(0)
                    .min((screen_h - popup_height).max(0));

                // Update state via JavaScript (preserves font cache - no reload flash)
                POPUP_WEBVIEW.with(|cell| {
                    if let Some(webview) = cell.borrow().as_ref() {
                        let update_script = generate_popup_update_script();
                        let _ = webview.evaluate_script(&update_script);
                    }
                });

                set_popup_bounds(hwnd, popup_x, popup_y);

                // Make fully visible (undo the warmup transparency)
                let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA);

                // Show and focus
                let _ = ShowWindow(hwnd, SW_SHOW);
                let _ = SetForegroundWindow(hwnd);

                // Start focus-polling timer
                let _ = SetTimer(Some(hwnd), 888, 100, None);

                LRESULT(0)
            }

            WM_ACTIVATE => LRESULT(0),

            WM_TIMER => {
                if wparam.0 == 888 {
                    // Focus polling: check if we're still the active window
                    let fg = GetForegroundWindow();
                    let root = GetAncestor(fg, GA_ROOT);

                    // If focus is on this popup or its children (WebView2), stay open
                    if fg == hwnd || root == hwnd {
                        return LRESULT(0);
                    }

                    // Focus is elsewhere - check grace period
                    let now = windows::Win32::System::SystemInformation::GetTickCount64();
                    if now > IGNORE_FOCUS_LOSS_UNTIL.load(Ordering::SeqCst) {
                        let _ = KillTimer(Some(hwnd), 888);
                        hide_tray_popup();
                    }
                }
                LRESULT(0)
            }

            WM_CLOSE => {
                // Just hide - don't destroy. Preserves WebView for instant redisplay.
                let _ = KillTimer(Some(hwnd), 888);
                let _ = ShowWindow(hwnd, SW_HIDE);
                LRESULT(0)
            }

            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }

            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}
