// --- TEXT INPUT WINDOW ---
// Window creation and WebView initialization.

use super::messages::input_wnd_proc;
use super::passive;
use super::state::*;
use super::styles::{HwndWrapper, get_editor_html};
use crate::win_types::SendHhook;
use std::sync::atomic::Ordering;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::{
    DWMWA_WINDOW_CORNER_PREFERENCE, DwmExtendFrameIntoClientArea, DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::Com::{CoInitialize, CoUninitialize};
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::*;
use wry::{Rect, WebViewBuilder};

/// The editor's physical size on a monitor: its authored CSS size at that
/// monitor's scale, kept inside the work area.
pub(super) fn editor_size(
    area: crate::overlay::display_placement::WorkArea,
    scale: f64,
) -> (i32, i32) {
    area.physical_size(560.0, 253.0, scale)
}

pub fn internal_create_window_loop() {
    unsafe {
        let coinit = CoInitialize(None); // Required for WebView
        crate::log_info!("[TextInput] Loop Start - CoInit: {:?}", coinit);
        let instance = GetModuleHandleW(None).unwrap();
        let class_name = w!("SGT_TextInputWry");

        REGISTER_INPUT_CLASS.call_once(|| {
            let wc = WNDCLASSW {
                lpfnWndProc: Some(input_wnd_proc),
                hInstance: instance.into(),
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap(),
                lpszClassName: class_name,
                style: CS_HREDRAW | CS_VREDRAW,
                // Use NULL brush to prevent white flashes/stripes on resize
                hbrBackground: HBRUSH(GetStockObject(NULL_BRUSH).0),
                ..Default::default()
            };
            let _ = RegisterClassW(&wc);
        });
        crate::log_info!("[TextInput] Class Registered");

        // Warm-up size only: every show resizes for the cursor's monitor.
        let scale = crate::overlay::display_placement::primary_scale();
        let (win_w, win_h) = editor_size(
            crate::overlay::display_placement::primary_work_area(),
            scale,
        );

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
            CoUninitialize();
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
            CoUninitialize();
            return;
        }

        if let Ok(hook) = SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(passive::keyboard_hook_proc),
            Some(instance.into()),
            0,
        ) {
            passive::seed_hook_keys();
            *INPUT_HOOK.lock().unwrap() = SendHhook(hook);
        }
        IS_WARMED_UP.store(true, Ordering::SeqCst);
        IS_WARMING_UP.store(false, Ordering::SeqCst);

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
        let hook = *INPUT_HOOK.lock().unwrap();
        if !hook.0.is_invalid() {
            let _ = UnhookWindowsHookEx(hook.0);
            *INPUT_HOOK.lock().unwrap() = SendHhook::default();
        }
        passive::retire_hook_keys();
        IS_WARMED_UP.store(false, Ordering::SeqCst);
        IS_WARMING_UP.store(false, Ordering::SeqCst);
        CoUninitialize();
    }
}

pub unsafe fn init_webview(hwnd: HWND, w: i32, h: i32) -> std::result::Result<(), ()> {
    let mut nonce_bytes = [0u8; 16];
    getrandom::fill(&mut nonce_bytes).map_err(|_| ())?;
    let document_nonce = nonce_bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let initialization_script = format!(
        "if (window.top === window) {{ window.__sgtDocumentNonce = '{}'; window.__sgtCancelledThrough = '0'; }}",
        document_nonce
    );
    // Use exact window dimensions for the webview, no insets.
    // The CSS .editor-container handles the padding/border-radius/shadow.
    let webview_x = 0;
    let webview_y = 0;
    let webview_w = w;
    let webview_h = h;

    let is_dark = if let Ok(app) = crate::APP.lock() {
        app.config.theme_mode.is_dark()
    } else {
        true
    };

    let placeholder = "Ready...";
    let initial_title = CFG_TITLE.lock().unwrap().clone();
    let html = get_editor_html(placeholder, is_dark, &initial_title);
    let wrapper = HwndWrapper(hwnd);

    // Initialize shared WebContext if needed
    TEXT_INPUT_WEB_CONTEXT.with(|ctx| {
        if ctx.borrow().is_none() {
            // Consolidate all minor overlays to 'common' to share one browser process and keep RAM at ~80MB
            *ctx.borrow_mut() = Some(crate::overlay::webview_runtime::create_context(
                crate::overlay::webview_runtime::Profile::Common,
            ));
        }
    });

    crate::log_info!("[TextInput] Starting WebView build phase...");

    let page_url = crate::overlay::html_components::font_manager::store_html_page(html.clone())
        .unwrap_or_else(|| format!("data:text/html,{}", urlencoding::encode(&html)));
    let page = crate::overlay::trusted_page::TrustedPage::new(&page_url).ok_or(())?;
    let result = {
        // LOCK SCOPE: Only one WebView builds at a time to prevent "Not enough quota"
        let init = crate::overlay::webview_init::acquire("text-input");
        crate::log_info!("[TextInput] Acquired init lock. Building...");

        let build_res = TEXT_INPUT_WEB_CONTEXT.with(|ctx| {
            let mut ctx_ref = ctx.borrow_mut();
            let builder = if let Some(web_ctx) = ctx_ref.as_mut() {
                WebViewBuilder::new_with_web_context(web_ctx)
            } else {
                WebViewBuilder::new()
            };
            let builder = builder.with_transparent(true);
            crate::log_info!("[TextInput] Builder configured. Preparing build...");

            let ipc_page = page.clone();
            let navigation_page = page.clone();

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
                .with_initialization_script(&initialization_script)
                .with_transparent(true)
                .with_ipc_handler(move |msg: wry::http::Request<String>| {
                    if ipc_page.allow_ipc(&msg.uri().to_string()) {
                        handle_ipc_message(msg.body());
                    }
                })
                .with_navigation_handler(move |url| navigation_page.allow_navigation(&url))
                .build_as_child(&wrapper)
        });
        crate::log_info!(
            "[TextInput] Build phase finished. Releasing lock. Status: {}",
            if build_res.is_ok() { "OK" } else { "ERR" }
        );
        init.finish(build_res.is_ok());
        build_res
    };

    if let Ok(webview) = result {
        *DOCUMENT_NONCE.lock().unwrap() = Some(document_nonce);
        crate::overlay::webview_diagnostics::attach_webview2_close_on_failure(
            "text-input",
            hwnd,
            &webview,
        );
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
    let Some((session, body)) = body.split_once(':') else {
        return;
    };
    let Ok(session) = session.parse::<u64>() else {
        return;
    };
    if !EDITOR_SESSIONS.is_current(session) {
        return;
    }
    if body.starts_with("submit:") {
        let Some((revision, text)) = body
            .strip_prefix("submit:")
            .and_then(|value| value.split_once(':'))
        else {
            return;
        };
        if revision.parse::<u64>().is_err() {
            return;
        }
        let text = text.to_string();
        if !text.trim().is_empty() {
            // Save to history before submitting
            crate::overlay::input_history::add_to_history(&text);
            *SUBMITTED_TEXT.lock().unwrap() = Some(SubmittedText {
                snapshot: text.clone(),
                text,
                revision: revision.to_string(),
                passive: false,
            });
            *SHOULD_CLOSE.lock().unwrap() = true;
        }
    } else if body == "cancel" {
        crate::overlay::input_history::reset_history_navigation();
        *SHOULD_CLOSE.lock().unwrap() = true;
    } else if body.starts_with("history_up:") {
        let current = body.strip_prefix("history_up:").unwrap_or("");
        if let Some(text) = crate::overlay::input_history::navigate_history_up(current) {
            super::queue_editor_text(session, text);
        }
    } else if body.starts_with("history_down:") {
        let current = body.strip_prefix("history_down:").unwrap_or("");
        if let Some(text) = crate::overlay::input_history::navigate_history_down(current) {
            super::queue_editor_text(session, text);
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
            let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
            crate::overlay::utils::begin_window_drag(hwnd);
        }
    } else if body == "close_window" {
        super::cancel_input();
    }
}
