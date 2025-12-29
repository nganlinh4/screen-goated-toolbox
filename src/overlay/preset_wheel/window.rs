// Preset Wheel Window - Persistent Hidden Window for Instant Appearance

use super::html::{generate_items_html, get_wheel_template};
use crate::config::Preset;
use crate::APP;
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicIsize, Ordering};
use std::sync::{Mutex, Once};
use windows::core::w;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;
use wry::{Rect, WebContext, WebView, WebViewBuilder};

static REGISTER_WHEEL_CLASS: Once = Once::new();
static REGISTER_OVERLAY_CLASS: Once = Once::new();

// Custom Messages
const WM_APP_SHOW: u32 = WM_USER + 10;
const WM_APP_HIDE: u32 = WM_USER + 11;

// Fixed dimensions for the wheel window to avoid resizing artifacts
// 800x600 is large enough to contain even the largest wheel configuration
const WHEEL_WIDTH: i32 = 800;
const WHEEL_HEIGHT: i32 = 600;

// Result communication
pub static WHEEL_RESULT: AtomicI32 = AtomicI32::new(-1); // -1 = pending, -2 = dismissed, >=0 = preset index
pub static WHEEL_ACTIVE: AtomicBool = AtomicBool::new(false);

// Thread-safe handles for the persistent windows - Using Atomics to satisfy Rust 2024
// 0 means invalid/null
static WHEEL_HWND: AtomicIsize = AtomicIsize::new(0);
static OVERLAY_HWND: AtomicIsize = AtomicIsize::new(0);

// Shared data for showing the wheel
lazy_static::lazy_static! {
    static ref PENDING_ITEMS_HTML: Mutex<String> = Mutex::new(String::new());
    static ref PENDING_DISMISS_LABEL: Mutex<String> = Mutex::new(String::new());
    static ref PENDING_POS: Mutex<(i32, i32)> = Mutex::new((0, 0)); // x, y
    static ref SELECTED_PRESET: Mutex<Option<usize>> = Mutex::new(None);
}

thread_local! {
    static WHEEL_WEBVIEW: RefCell<Option<WebView>> = RefCell::new(None);
    static WHEEL_WEB_CONTEXT: RefCell<Option<WebContext>> = RefCell::new(None);
}

// HWND wrapper for wry
struct HwndWrapper(HWND);
unsafe impl Send for HwndWrapper {}
unsafe impl Sync for HwndWrapper {}
impl raw_window_handle::HasWindowHandle for HwndWrapper {
    fn window_handle(
        &self,
    ) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        let raw = raw_window_handle::Win32WindowHandle::new(
            std::num::NonZeroIsize::new(self.0 .0 as isize).expect("HWND cannot be null"),
        );
        let handle = raw_window_handle::RawWindowHandle::Win32(raw);
        unsafe { Ok(raw_window_handle::WindowHandle::borrow_raw(handle)) }
    }
}

/// Pre-initialize the persistent window thread
pub fn warmup() {
    std::thread::spawn(|| {
        internal_create_window_loop();
    });
}

/// Show preset wheel and return selected preset index (or None if dismissed)
/// This function blocks until user makes a selection
pub fn show_preset_wheel(
    filter_type: &str,
    filter_mode: Option<&str>,
    center_pos: POINT,
) -> Option<usize> {
    unsafe {
        // Reset state
        WHEEL_RESULT.store(-1, Ordering::SeqCst);
        WHEEL_ACTIVE.store(true, Ordering::SeqCst);
        *SELECTED_PRESET.lock().unwrap() = None;

        // Get filtered presets
        let (presets, ui_lang) = {
            let app = APP.lock().unwrap();
            (app.config.presets.clone(), app.config.ui_language.clone())
        };

        // Filter presets based on type and mode
        let filtered: Vec<(usize, Preset)> = presets
            .iter()
            .enumerate()
            .filter(|(_, p)| {
                if p.is_master {
                    return false;
                }
                if p.is_upcoming {
                    return false;
                }
                if p.preset_type != filter_type {
                    return false;
                }
                if filter_type == "audio" && p.audio_processing_mode == "realtime" {
                    return false;
                }
                if let Some(mode) = filter_mode {
                    match filter_type {
                        "text" => {
                            if p.text_input_mode != mode {
                                return false;
                            }
                        }
                        "audio" => {
                            if p.audio_source != mode {
                                return false;
                            }
                        }
                        _ => {}
                    }
                }
                true
            })
            .map(|(i, p)| (i, p.clone()))
            .collect();

        if filtered.is_empty() {
            WHEEL_ACTIVE.store(false, Ordering::SeqCst);
            return None;
        }

        // Dismiss label
        let dismiss_label = match ui_lang.as_str() {
            "vi" => "HỦY",
            "ko" => "취소",
            _ => "CANCEL",
        };

        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let screen_h = GetSystemMetrics(SM_CYSCREEN);

        // Center the 800x600 window on the cursor/center_pos
        let win_x = (center_pos.x - WHEEL_WIDTH / 2)
            .max(0)
            .min(screen_w - WHEEL_WIDTH);
        let win_y = (center_pos.y - WHEEL_HEIGHT / 2)
            .max(0)
            .min(screen_h - WHEEL_HEIGHT);

        // Prepare content
        let items_html = generate_items_html(&filtered, &ui_lang);

        // Store data for the UI thread
        *PENDING_ITEMS_HTML.lock().unwrap() = items_html;
        *PENDING_DISMISS_LABEL.lock().unwrap() = dismiss_label.to_string();
        *PENDING_POS.lock().unwrap() = (win_x, win_y);

        // Wake up the persistent window
        let hwnd_val = WHEEL_HWND.load(Ordering::SeqCst);
        let wheel_hwnd = HWND(hwnd_val as *mut _);

        if !wheel_hwnd.is_invalid() {
            let _ = PostMessageW(Some(wheel_hwnd), WM_APP_SHOW, WPARAM(0), LPARAM(0));
        } else {
            // Cold start fallback
            warmup();
            std::thread::sleep(std::time::Duration::from_millis(100));
            // Check again
            let hwnd_val = WHEEL_HWND.load(Ordering::SeqCst);
            let wheel_hwnd = HWND(hwnd_val as *mut _);
            if !wheel_hwnd.is_invalid() {
                let _ = PostMessageW(Some(wheel_hwnd), WM_APP_SHOW, WPARAM(0), LPARAM(0));
            }
        }

        // Message loop waiting for selection (blocking the caller)
        let mut msg = MSG::default();
        loop {
            // We must pump messages allow IPC/other stuff
            let res = WHEEL_RESULT.load(Ordering::SeqCst);
            if res != -1 {
                break;
            }
            if PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        WHEEL_ACTIVE.store(false, Ordering::SeqCst);
        let res = WHEEL_RESULT.load(Ordering::SeqCst);
        if res >= 0 {
            Some(res as usize)
        } else {
            None
        }
    }
}

pub fn dismiss_wheel() {
    unsafe {
        let hwnd_val = WHEEL_HWND.load(Ordering::SeqCst);
        let wheel_hwnd = HWND(hwnd_val as *mut _);
        if !wheel_hwnd.is_invalid() {
            let _ = PostMessageW(Some(wheel_hwnd), WM_APP_HIDE, WPARAM(0), LPARAM(0));
        }
    }
    WHEEL_RESULT.store(-2, Ordering::SeqCst);
}

pub fn is_wheel_active() -> bool {
    WHEEL_ACTIVE.load(Ordering::SeqCst)
}

fn internal_create_window_loop() {
    unsafe {
        let instance = GetModuleHandleW(None).unwrap_or_default();
        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let screen_h = GetSystemMetrics(SM_CYSCREEN);

        // --- 1. Create Overlay Window ---
        let overlay_class = w!("SGTWheelOverlayPersistent");
        REGISTER_OVERLAY_CLASS.call_once(|| {
            let wc = WNDCLASSW {
                lpfnWndProc: Some(overlay_wnd_proc),
                hInstance: instance.into(),
                lpszClassName: overlay_class,
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                hbrBackground: HBRUSH(std::ptr::null_mut()),
                ..Default::default()
            };
            RegisterClassW(&wc);
        });

        let overlay_hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED | WS_EX_NOACTIVATE,
            overlay_class,
            w!("WheelOverlay"),
            WS_POPUP, // Initially hidden
            0,
            0,
            screen_w,
            screen_h,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .unwrap_or_default();

        OVERLAY_HWND.store(overlay_hwnd.0 as isize, Ordering::SeqCst);
        let _ = SetLayeredWindowAttributes(overlay_hwnd, COLORREF(0), 1, LWA_ALPHA);

        // --- 2. Create Wheel Window (Fixed Size) ---
        let class_name = w!("SGTPresetWheelPersistent");
        REGISTER_WHEEL_CLASS.call_once(|| {
            let wc = WNDCLASSW {
                lpfnWndProc: Some(wheel_wnd_proc),
                hInstance: instance.into(),
                lpszClassName: class_name,
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                hbrBackground: HBRUSH(std::ptr::null_mut()),
                ..Default::default()
            };
            RegisterClassW(&wc);
        });

        // Create initially VISIBLE but OFF-SCREEN to ensure GPU context is happy for transparency
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED,
            class_name,
            w!("PresetWheel"),
            WS_POPUP | WS_VISIBLE,
            -4000,
            -4000,
            WHEEL_WIDTH,
            WHEEL_HEIGHT,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .unwrap_or_default();

        WHEEL_HWND.store(hwnd.0 as isize, Ordering::SeqCst);
        let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA);

        // --- 3. Initialize WebView ---
        let wrapper = HwndWrapper(hwnd);

        WHEEL_WEB_CONTEXT.with(|ctx| {
            if ctx.borrow().is_none() {
                let shared_data_dir = crate::overlay::get_shared_webview_data_dir();
                *ctx.borrow_mut() = Some(WebContext::new(Some(shared_data_dir)));
            }
        });

        let webview_res = WHEEL_WEB_CONTEXT.with(|ctx| {
            let mut ctx_ref = ctx.borrow_mut();
            let builder = if let Some(web_ctx) = ctx_ref.as_mut() {
                WebViewBuilder::new_with_web_context(web_ctx)
            } else {
                WebViewBuilder::new()
            };
            let builder = crate::overlay::html_components::font_manager::configure_webview(builder);

            // Generate STATIC template once
            let template_html = get_wheel_template();

            builder
                .with_transparent(true)
                .with_background_color((0, 0, 0, 0)) // Explicit transparent background
                .with_html(template_html) // Load skeleton ONCE at startup
                .with_bounds(Rect {
                    position: wry::dpi::Position::Physical(wry::dpi::PhysicalPosition::new(0, 0)),
                    size: wry::dpi::Size::Physical(wry::dpi::PhysicalSize::new(
                        WHEEL_WIDTH as u32,
                        WHEEL_HEIGHT as u32,
                    )),
                })
                .with_ipc_handler(move |msg: wry::http::Request<String>| {
                    let body = msg.body();
                    if body == "dismiss" {
                        let hwnd_val = WHEEL_HWND.load(Ordering::SeqCst);
                        let wheel_hwnd = HWND(hwnd_val as *mut _);
                        if !wheel_hwnd.is_invalid() {
                            let _ =
                                PostMessageW(Some(wheel_hwnd), WM_APP_HIDE, WPARAM(0), LPARAM(0));
                        }
                        *SELECTED_PRESET.lock().unwrap() = None;
                        WHEEL_RESULT.store(-2, Ordering::SeqCst);
                    } else if let Some(idx_str) = body.strip_prefix("select:") {
                        if let Ok(idx) = idx_str.parse::<usize>() {
                            let hwnd_val = WHEEL_HWND.load(Ordering::SeqCst);
                            let wheel_hwnd = HWND(hwnd_val as *mut _);
                            if !wheel_hwnd.is_invalid() {
                                let _ = PostMessageW(
                                    Some(wheel_hwnd),
                                    WM_APP_HIDE,
                                    WPARAM(0),
                                    LPARAM(0),
                                );
                            }
                            *SELECTED_PRESET.lock().unwrap() = Some(idx);
                            WHEEL_RESULT.store(idx as i32, Ordering::SeqCst);
                        }
                    }
                })
                .build(&wrapper)
        });

        if let Ok(wv) = webview_res {
            WHEEL_WEBVIEW.with(|cell| {
                *cell.borrow_mut() = Some(wv);
            });
            // HIDE the window now that WebView is ready
            let _ = ShowWindow(hwnd, SW_HIDE);
        }

        // --- 4. Message Loop ---
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        // Cleanup
        WHEEL_WEBVIEW.with(|cell| {
            *cell.borrow_mut() = None;
        });
        WHEEL_HWND.store(0, Ordering::SeqCst);
        OVERLAY_HWND.store(0, Ordering::SeqCst);
    }
}

unsafe extern "system" fn overlay_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_LBUTTONDOWN | WM_RBUTTONDOWN => {
            // Click on overlay = dismiss
            let hwnd_val = WHEEL_HWND.load(Ordering::SeqCst);
            let wheel_hwnd = HWND(hwnd_val as *mut _);
            if !wheel_hwnd.is_invalid() {
                let _ = PostMessageW(Some(wheel_hwnd), WM_APP_HIDE, WPARAM(0), LPARAM(0));
            }
            WHEEL_RESULT.store(-2, Ordering::SeqCst);
            LRESULT(0)
        }
        WM_CLOSE => LRESULT(0), // Ignore close, we hide instead
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe extern "system" fn wheel_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_APP_SHOW => {
            // 1. Get parameters
            let (x, y) = *PENDING_POS.lock().unwrap();
            let items_html = PENDING_ITEMS_HTML.lock().unwrap().clone();
            let dismiss_label = PENDING_DISMISS_LABEL.lock().unwrap().clone();

            // 2. Move Wheel (NO SIZE CHANGE)
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                x,
                y,
                0,
                0,
                SWP_NOACTIVATE | SWP_NOSIZE, // Important: NOSIZE
            );

            // 3. Update WebView Content via JS Injection without resizing
            WHEEL_WEBVIEW.with(|wv| {
                if let Some(webview) = wv.borrow().as_ref() {
                    // Just update content
                    let script = format!(
                        "window.updateContent(`{}`, `{}`);",
                        items_html
                            .replace("\\", "\\\\")
                            .replace("`", "\\`")
                            .replace("$", "\\$"),
                        dismiss_label.replace("`", "\\`").replace("$", "\\$")
                    );
                    let _ = webview.evaluate_script(&script);
                }
            });

            // 4. Force repaint (optional since size didn't change, but good hygiene)
            let _ = InvalidateRect(Some(hwnd), None, true);

            // 5. Show Windows
            let overlay_val = OVERLAY_HWND.load(Ordering::SeqCst);
            let overlay = HWND(overlay_val as *mut _);
            if !overlay.is_invalid() {
                let _ = ShowWindow(overlay, SW_SHOWNOACTIVATE);
                // Overlay covers full screen
                let screen_w = GetSystemMetrics(SM_CXSCREEN);
                let screen_h = GetSystemMetrics(SM_CYSCREEN);
                let _ = SetWindowPos(
                    overlay,
                    Some(HWND_TOPMOST),
                    0,
                    0,
                    screen_w,
                    screen_h,
                    SWP_NOACTIVATE | SWP_NOMOVE,
                );
            }

            // Show wheel
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE,
            );

            LRESULT(0)
        }

        WM_APP_HIDE => {
            let _ = ShowWindow(hwnd, SW_HIDE);
            let overlay_val = OVERLAY_HWND.load(Ordering::SeqCst);
            let overlay = HWND(overlay_val as *mut _);
            if !overlay.is_invalid() {
                let _ = ShowWindow(overlay, SW_HIDE);
            }
            LRESULT(0)
        }

        WM_KEYDOWN => {
            if wparam.0 as u32 == 0x1B {
                // Escape
                let _ = PostMessageW(Some(hwnd), WM_APP_HIDE, WPARAM(0), LPARAM(0));
                WHEEL_RESULT.store(-2, Ordering::SeqCst);
            }
            LRESULT(0)
        }

        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }

        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
