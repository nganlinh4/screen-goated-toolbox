// Tray Popup - Custom non-blocking popup window for tray icon menu
// Replaces native Windows tray context menu to avoid blocking the main UI thread

use crate::APP;
use std::cell::RefCell;
use std::sync::{
    atomic::{AtomicBool, AtomicIsize, Ordering},
    Once,
};
use windows::core::w;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;
use wry::{Rect, WebView, WebViewBuilder};

static REGISTER_POPUP_CLASS: Once = Once::new();
static POPUP_ACTIVE: AtomicBool = AtomicBool::new(false);
static POPUP_HWND: AtomicIsize = AtomicIsize::new(0);

thread_local! {
    static POPUP_WEBVIEW: RefCell<Option<WebView>> = RefCell::new(None);
}

const POPUP_WIDTH: i32 = 250;
const POPUP_HEIGHT: i32 = 160;

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

/// Show the tray popup at cursor position
pub fn show_tray_popup() {
    // Prevent duplicates
    if POPUP_ACTIVE.swap(true, Ordering::SeqCst) {
        // Already active - close it instead (toggle behavior)
        hide_tray_popup();
        return;
    }

    std::thread::spawn(|| {
        create_popup_window();
    });
}

/// Hide the tray popup
pub fn hide_tray_popup() {
    if !POPUP_ACTIVE.load(Ordering::SeqCst) {
        return;
    }

    let hwnd_val = POPUP_HWND.load(Ordering::SeqCst);
    if hwnd_val != 0 {
        let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
        unsafe {
            let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }
}

/// Warmup the WebView2 runtime by creating a hidden popup and immediately closing it
/// Call this at app startup to ensure fast popup display later
pub fn warmup_tray_popup() {
    // Disabled to prevent startup focus stealing and race conditions.
    // The popup will initialize on first use.
}

fn generate_popup_html() -> String {
    let (settings_text, bubble_text, quit_text, bubble_checked) = if let Ok(app) = APP.lock() {
        let lang = &app.config.ui_language;
        let settings = match lang.as_str() {
            "vi" => "⚙️ Cài đặt",
            "ko" => "⚙️ 설정",
            _ => "⚙️ Settings",
        };
        let bubble = match lang.as_str() {
            "vi" => "⭐ Hiển thị bong bóng",
            "ko" => "⭐ 즐겨찾기 버블",
            _ => "⭐ Favorite Bubble",
        };
        let quit = match lang.as_str() {
            "vi" => "❌ Thoát",
            "ko" => "❌ 종료",
            _ => "❌ Quit",
        };
        let checked = app.config.show_favorite_bubble;
        (settings, bubble, quit, checked)
    } else {
        ("⚙️ Settings", "⭐ Favorite Bubble", "❌ Quit", false)
    };

    let check_mark = if bubble_checked { "✓ " } else { "" };

    format!(
        r#"<!DOCTYPE html>
<html>
<head>
<meta charset="UTF-8">
<style>
* {{ margin: 0; padding: 0; box-sizing: border-box; }}
html, body {{
    width: 100%;
    height: 100%;
    overflow: hidden;
    background: #1e1e28;
    font-family: 'Segoe UI', system-ui, sans-serif;
    user-select: none;
    border: 1px solid #444;
    border-radius: 8px;
}}

.container {{
    display: flex;
    flex-direction: column;
    padding: 6px;
}}

.menu-item {{
    padding: 10px 14px;
    border-radius: 6px;
    cursor: pointer;
    color: white;
    font-size: 13px;
    margin-bottom: 2px;
    background: transparent;
    transition: all 0.1s ease;
}}

.menu-item:hover {{
    background: rgba(102, 126, 234, 0.4);
}}

.menu-item.quit:hover {{
    background: rgba(220, 80, 80, 0.4);
}}

.separator {{
    height: 1px;
    background: rgba(255,255,255,0.1);
    margin: 4px 8px;
}}
</style>
</head>
<body>
<div class="container">
    <div class="menu-item" onclick="action('settings')">{settings}</div>
    <div class="menu-item" onclick="action('bubble')"><span id="bubble-check">{check}</span>{bubble}</div>
    <div class="separator"></div>
    <div class="menu-item quit" onclick="action('quit')">{quit}</div>
</div>
<script>
function action(cmd) {{
    window.ipc.postMessage(cmd);
}}
// Close on click outside (detect blur)
window.addEventListener('blur', function() {{
    window.ipc.postMessage('close');
}});
</script>
</body>
</html>"#,
        settings = settings_text,
        bubble = bubble_text,
        quit = quit_text,
        check = check_mark
    )
}

fn create_popup_window() {
    unsafe {
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

        // Get cursor position for placement
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);

        // Position popup above and to the left of cursor (typical tray menu behavior)
        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let screen_h = GetSystemMetrics(SM_CYSCREEN);

        let popup_x = (pt.x - POPUP_WIDTH / 2).max(0).min(screen_w - POPUP_WIDTH);
        let popup_y = (pt.y - POPUP_HEIGHT - 10)
            .max(0)
            .min(screen_h - POPUP_HEIGHT);

        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            class_name,
            w!("TrayPopup"),
            WS_POPUP,
            popup_x,
            popup_y,
            POPUP_WIDTH,
            POPUP_HEIGHT,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .unwrap_or_default();

        if hwnd.is_invalid() {
            POPUP_ACTIVE.store(false, Ordering::SeqCst);
            return;
        }

        POPUP_HWND.store(hwnd.0 as isize, Ordering::SeqCst);

        // Round corners
        let corner_pref = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            std::ptr::addr_of!(corner_pref) as *const _,
            std::mem::size_of_val(&corner_pref) as u32,
        );

        // Create WebView
        let wrapper = HwndWrapper(hwnd);
        let html = generate_popup_html();

        let webview = WebViewBuilder::new()
            .with_bounds(Rect {
                position: wry::dpi::Position::Logical(wry::dpi::LogicalPosition::new(0.0, 0.0)),
                size: wry::dpi::Size::Physical(wry::dpi::PhysicalSize::new(
                    POPUP_WIDTH as u32,
                    POPUP_HEIGHT as u32,
                )),
            })
            .with_transparent(true)
            .with_html(&html)
            .with_ipc_handler(move |msg: wry::http::Request<String>| {
                let body = msg.body();
                match body.as_str() {
                    "settings" => {
                        // Restore main window
                        let h = POPUP_HWND.load(Ordering::SeqCst);
                        if h != 0 {
                            let _ = PostMessageW(
                                Some(HWND(h as *mut _)),
                                WM_CLOSE,
                                WPARAM(0),
                                LPARAM(0),
                            );
                        }
                        // Signal to open settings
                        crate::gui::signal_restore_window();
                    }
                    "bubble" => {
                        // Toggle bubble - don't close popup, just update the checkmark
                        let new_state = if let Ok(mut app) = APP.lock() {
                            app.config.show_favorite_bubble = !app.config.show_favorite_bubble;
                            let enabled = app.config.show_favorite_bubble;
                            crate::config::save_config(&app.config);

                            if enabled {
                                crate::overlay::favorite_bubble::show_favorite_bubble();
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
                                    "document.getElementById('bubble-check').textContent = '{}';",
                                    if new_state { "✓ " } else { "" }
                                );
                                let _ = webview.evaluate_script(&js);
                            }
                        });
                    }
                    "quit" => {
                        // Close popup first
                        let h = POPUP_HWND.load(Ordering::SeqCst);
                        if h != 0 {
                            let _ = PostMessageW(
                                Some(HWND(h as *mut _)),
                                WM_CLOSE,
                                WPARAM(0),
                                LPARAM(0),
                            );
                        }
                        // Small delay to let window close, then exit
                        std::thread::spawn(|| {
                            std::thread::sleep(std::time::Duration::from_millis(50));
                            std::process::exit(0);
                        });
                    }
                    "close" => {
                        let h = POPUP_HWND.load(Ordering::SeqCst);
                        if h != 0 {
                            let _ = PostMessageW(
                                Some(HWND(h as *mut _)),
                                WM_CLOSE,
                                WPARAM(0),
                                LPARAM(0),
                            );
                        }
                    }
                    _ => {}
                }
            })
            .build(&wrapper);

        if let Ok(wv) = webview {
            POPUP_WEBVIEW.with(|cell| {
                *cell.borrow_mut() = Some(wv);
            });
        }

        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        let _ = SetForegroundWindow(hwnd);

        // Message loop
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        // Cleanup
        POPUP_WEBVIEW.with(|cell| {
            *cell.borrow_mut() = None;
        });
        POPUP_ACTIVE.store(false, Ordering::SeqCst);
        POPUP_HWND.store(0, Ordering::SeqCst);
    }
}

unsafe extern "system" fn popup_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_ACTIVATE => {
            // Close when deactivated (clicked outside)
            if wparam.0 as u32 == 0 {
                // WA_INACTIVE
                let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
            LRESULT(0)
        }

        WM_CLOSE => {
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }

        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }

        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
