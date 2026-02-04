// --- TEXT INPUT WINDOW ---
// Window creation and WebView initialization.

use super::messages::input_wnd_proc;
use super::state::*;
use super::styles::{get_editor_html, HwndWrapper};
use std::sync::atomic::Ordering;
use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::{
    DwmExtendFrameIntoClientArea, DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE,
};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::Com::{CoInitialize, CoUninitialize};
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::HiDpi::GetDpiForSystem;
use windows::Win32::UI::WindowsAndMessaging::*;
use wry::{Rect, WebContext, WebViewBuilder};

pub fn internal_create_window_loop() {
    unsafe {
        let coinit = CoInitialize(None); // Required for WebView
        crate::log_info!("[TextInput] Loop Start - CoInit: {:?}", coinit);
        let instance = GetModuleHandleW(None).unwrap();
        let class_name = w!("SGT_TextInputWry");

        REGISTER_INPUT_CLASS.call_once(|| {
            let mut wc = WNDCLASSW::default();
            wc.lpfnWndProc = Some(input_wnd_proc);
            wc.hInstance = instance.into();
            wc.hCursor = LoadCursorW(None, IDC_ARROW).unwrap();
            wc.lpszClassName = class_name;
            wc.style = CS_HREDRAW | CS_VREDRAW;
            // Use NULL brush to prevent white flashes/stripes on resize
            wc.hbrBackground = HBRUSH(GetStockObject(NULL_BRUSH).0);
            let _ = RegisterClassW(&wc);
        });
        crate::log_info!("[TextInput] Class Registered");

        crate::log_info!("[TextInput] Calculating scale...");
        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let scale = {
            let dpi = GetDpiForSystem();
            crate::log_info!("[TextInput] System DPI: {}", dpi);
            dpi as f64 / 96.0
        };
        // Width scaling: matches 800px physical at 1.25 scale (Laptop preferred),
        // but creates ~580px physical at 1.0 scale (PC preferred, narrower than 640).
        let mut win_w = ((880.0 * scale) - 300.0).round() as i32;

        // User requested smaller width for 1920x1080 laptops (usually scale > 1.0)
        // Current formula gives 800px at 1.25 scale, which is too wide.
        // We reduce it by 15% to ~680px for this specific case.
        if screen_w == 1920 && scale > 1.1 {
            win_w = (win_w as f64 * 0.85).round() as i32;
        }
        let win_h = (253.0 * scale).round() as i32;

        crate::log_info!(
            "[TextInput] Creating window: scale={:.2}, width={}, height={}",
            scale,
            win_w,
            win_h
        );

        // Start HIDDEN logic (Offscreen but VISIBLE for Webview init)
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED,
            class_name,
            w!("Text Input"),
            WS_POPUP | WS_VISIBLE,
            -4000,
            -4000,
            win_w,
            win_h,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .unwrap_or_default();
        crate::log_info!("[TextInput] CreateWindowExW returned HWND: {:?}", hwnd);

        if hwnd.is_invalid() {
            crate::log_info!("[TextInput] Critical Error: Failed to create window.");
            IS_WARMED_UP.store(false, Ordering::SeqCst);
            IS_WARMING_UP.store(false, Ordering::SeqCst);
            let _ = CoUninitialize();
            return;
        }

        INPUT_HWND.store(hwnd.0 as isize, Ordering::SeqCst);

        // Windows 11 Rounded Corners - Disable native rounding
        let corner_pref = 1u32; // DWMWCP_DONOTROUND
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            std::ptr::addr_of!(corner_pref) as *const _,
            std::mem::size_of_val(&corner_pref) as u32,
        );
        crate::log_info!("[TextInput] HWND stored, starting WebView initialization...");

        // WebView Initialization
        // Initialize use simple DwmExtendFrameIntoClientArea for full transparency
        // NO SetLayeredWindowAttributes(hwnd, COLORREF(0), 0, LWA_COLORKEY) as it conflicts with Dwm
        // Use margins -1 to extend glass effect to entire window (fully transparent client area)
        let margins = MARGINS {
            cxLeftWidth: -1,
            cxRightWidth: -1,
            cyTopHeight: -1,
            cyBottomHeight: -1,
        };
        let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);

        // REMOVED GDI REGION CLIPPING
        // We now rely on HTML/CSS border-radius and transparent background

        // Create webview with retry logic
        let mut attempts = 0;
        let max_attempts = 3;
        let mut webview_success = false;

        while attempts < max_attempts {
            if init_webview(hwnd, win_w, win_h).is_ok() {
                webview_success = true;
                break;
            }
            crate::log_info!(
                "[TextInput] WebView init attempt {} failed, retrying...",
                attempts + 1
            );
            attempts += 1;
            crate::log_info!(
                "[TextInput] WebView init failed, retrying ({}/{})",
                attempts,
                max_attempts
            );
            std::thread::sleep(std::time::Duration::from_millis(500));
        }

        if !webview_success {
            crate::log_info!(
                "[TextInput] Critical Error: Failed to initialize WebView after {} attempts.",
                max_attempts
            );
            // Don't mark as warmed up, let it fail so show() can re-trigger warmup if needed
            IS_WARMED_UP.store(false, Ordering::SeqCst);
            IS_WARMING_UP.store(false, Ordering::SeqCst);
            let _ = DestroyWindow(hwnd);
            let _ = CoUninitialize();
            return;
        }

        // Mark as warmed up and ready
        IS_WARMED_UP.store(true, Ordering::SeqCst);
        IS_WARMING_UP.store(false, Ordering::SeqCst); // Done warming up

        // Message Loop
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            let _ = DispatchMessageW(&msg);
        }

        // Cleanup on exit
        TEXT_INPUT_WEBVIEW.with(|wv| {
            *wv.borrow_mut() = None;
        });
        INPUT_HWND.store(0, Ordering::SeqCst);
        IS_WARMED_UP.store(false, Ordering::SeqCst);
        IS_WARMING_UP.store(false, Ordering::SeqCst);
        let _ = CoUninitialize();
    }
}

pub unsafe fn init_webview(hwnd: HWND, w: i32, h: i32) -> std::result::Result<(), ()> {
    // Use exact window dimensions for the webview, no insets.
    // The CSS .editor-container handles the padding/border-radius/shadow.
    let webview_x = 0;
    let webview_y = 0;
    let webview_w = w;
    let webview_h = h;

    let is_dark = if let Ok(app) = crate::APP.lock() {
        match app.config.theme_mode {
            crate::config::ThemeMode::Dark => true,
            crate::config::ThemeMode::Light => false,
            crate::config::ThemeMode::System => crate::gui::utils::is_system_in_dark_mode(),
        }
    } else {
        true
    };

    let placeholder = "Ready...";
    let html = get_editor_html(placeholder, is_dark);
    let wrapper = HwndWrapper(hwnd);

    // Initialize shared WebContext if needed
    TEXT_INPUT_WEB_CONTEXT.with(|ctx| {
        if ctx.borrow().is_none() {
            // Consolidate all minor overlays to 'common' to share one browser process and keep RAM at ~80MB
            let shared_data_dir = crate::overlay::get_shared_webview_data_dir(Some("common"));
            *ctx.borrow_mut() = Some(WebContext::new(Some(shared_data_dir)));
        }
    });

    crate::log_info!("[TextInput] Starting WebView build phase...");

    let result = {
        // LOCK SCOPE: Only one WebView builds at a time to prevent "Not enough quota"
        let _init_lock = crate::overlay::GLOBAL_WEBVIEW_MUTEX.lock().unwrap();
        crate::log_info!("[TextInput] Acquired init lock. Building...");

        let build_res = TEXT_INPUT_WEB_CONTEXT.with(|ctx| {
            let mut ctx_ref = ctx.borrow_mut();
            let builder = if let Some(web_ctx) = ctx_ref.as_mut() {
                WebViewBuilder::new_with_web_context(web_ctx)
            } else {
                WebViewBuilder::new()
            };
            let builder = builder.with_transparent(true);
            let builder = crate::overlay::html_components::font_manager::configure_webview(builder);
            crate::log_info!("[TextInput] Builder configured. Preparing build...");

            let page_url =
                crate::overlay::html_components::font_manager::store_html_page(html.clone())
                    .unwrap_or_else(|| format!("data:text/html,{}", urlencoding::encode(&html)));

            crate::log_info!("[TextInput] URL prepared. Invoking build...");

            builder
                // Store HTML in font server and get URL for same-origin font loading
                .with_background_color((0, 0, 0, 0))
                .with_bounds(Rect {
                    position: wry::dpi::Position::Physical(wry::dpi::PhysicalPosition::new(
                        webview_x, webview_y,
                    )),
                    size: wry::dpi::Size::Physical(wry::dpi::PhysicalSize::new(
                        webview_w as u32,
                        webview_h as u32,
                    )),
                })
                .with_url(&page_url)
                .with_transparent(true)
                .with_ipc_handler(move |msg: wry::http::Request<String>| {
                    handle_ipc_message(msg.body());
                })
                .build_as_child(&wrapper)
        });
        crate::log_info!(
            "[TextInput] Build phase finished. Releasing lock. Status: {}",
            if build_res.is_ok() { "OK" } else { "ERR" }
        );
        build_res
    };

    if let Ok(webview) = result {
        println!("[TextInput] WebView initialization SUCCESSFUL");
        TEXT_INPUT_WEBVIEW.with(|wv| {
            *wv.borrow_mut() = Some(webview);
        });
        Ok(())
    } else {
        Err(())
    }
}

/// Handle IPC messages from WebView
fn handle_ipc_message(body: &str) {
    if body.starts_with("submit:") {
        let text = body.strip_prefix("submit:").unwrap_or("").to_string();
        if !text.trim().is_empty() {
            // Save to history before submitting
            crate::overlay::input_history::add_to_history(&text);
            *SUBMITTED_TEXT.lock().unwrap() = Some(text);
            *SHOULD_CLOSE.lock().unwrap() = true;
        }
    } else if body == "cancel" {
        crate::overlay::input_history::reset_history_navigation();
        *SHOULD_CLOSE.lock().unwrap() = true;
    } else if body.starts_with("history_up:") {
        let current = body.strip_prefix("history_up:").unwrap_or("");
        if let Some(text) = crate::overlay::input_history::navigate_history_up(current) {
            *PENDING_TEXT.lock().unwrap() = Some(format!("__REPLACE_ALL__{}", text));
            let hwnd_val = INPUT_HWND.load(Ordering::SeqCst);
            if hwnd_val != 0 {
                unsafe {
                    let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                        Some(HWND(hwnd_val as *mut std::ffi::c_void)),
                        WM_APP_SET_TEXT,
                        windows::Win32::Foundation::WPARAM(0),
                        windows::Win32::Foundation::LPARAM(0),
                    );
                }
            }
        }
    } else if body.starts_with("history_down:") {
        let current = body.strip_prefix("history_down:").unwrap_or("");
        if let Some(text) = crate::overlay::input_history::navigate_history_down(current) {
            *PENDING_TEXT.lock().unwrap() = Some(format!("__REPLACE_ALL__{}", text));
            let hwnd_val = INPUT_HWND.load(Ordering::SeqCst);
            if hwnd_val != 0 {
                unsafe {
                    let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                        Some(HWND(hwnd_val as *mut std::ffi::c_void)),
                        WM_APP_SET_TEXT,
                        windows::Win32::Foundation::WPARAM(0),
                        windows::Win32::Foundation::LPARAM(0),
                    );
                }
            }
        }
    } else if body == "mic" {
        // Trigger transcription preset
        let transcribe_idx = {
            let app = crate::APP.lock().unwrap();
            app.config
                .presets
                .iter()
                .position(|p| p.id == "preset_transcribe")
        };

        if let Some(preset_idx) = transcribe_idx {
            std::thread::spawn(move || {
                crate::overlay::recording::show_recording_overlay(preset_idx);
            });
        }
    } else if body == "drag_window" {
        let hwnd_val = INPUT_HWND.load(Ordering::SeqCst);
        if hwnd_val != 0 {
            unsafe {
                let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
                let _ = windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture();
                let _ = windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                    hwnd,
                    WM_NCLBUTTONDOWN,
                    Some(WPARAM(HTCAPTION as usize)),
                    Some(LPARAM(0)),
                );
            }
        }
    } else if body == "close_window" {
        super::cancel_input();
    }
}
