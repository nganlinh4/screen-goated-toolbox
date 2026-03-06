// --- SCREEN RECORD MODULE ---
// Screen recording overlay with WebView interface.

pub(crate) mod bg_download;
mod embedded_assets;
mod ipc;
mod raw_video;

use raw_window_handle::{
    HandleError, HasWindowHandle, RawWindowHandle, Win32WindowHandle, WindowHandle,
};
use serde::Deserialize;
use std::borrow::Cow;
use std::num::NonZeroIsize;
use std::path::{Path, PathBuf};
use std::sync::{Once, OnceLock};
use std::thread;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::{
    DWMWA_EXTENDED_FRAME_BOUNDS, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
    DwmGetWindowAttribute, DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Gdi::{
    CreateSolidBrush, GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::*;
use wry::{Rect, WebContext, WebViewBuilder};

use crate::{assets::GOOGLE_SANS_FLEX, win_types::SendHwnd};

pub mod audio_engine;
pub mod capture_border;
mod d3d_interop;
pub mod engine;
pub mod gpu_export;
mod gpu_pipeline;
pub mod input_capture;
pub mod mf_audio;
mod mf_decode;
mod mf_encode;
pub mod native_export;
pub mod window_selection;

// Re-exports
use ipc::handle_ipc_command;

// --- CONSTANTS ---
const WM_APP_SHOW: u32 = WM_USER + 110;
const WM_APP_TOGGLE: u32 = WM_USER + 111;
const WM_APP_RUN_SCRIPT: u32 = WM_USER + 112;
const WM_APP_UPDATE_SETTINGS: u32 = WM_USER + 113;

// --- STATE ---
static REGISTER_SR_CLASS: Once = Once::new();
pub static mut SR_HWND: SendHwnd = SendHwnd(HWND(std::ptr::null_mut()));
/// Saved window rect before entering video fullscreen, to restore on exit.
static PRE_FULLSCREEN_RECT: std::sync::Mutex<Option<(i32, i32, i32, i32)>> =
    std::sync::Mutex::new(None);
static mut IS_WARMED_UP: bool = false;
static mut IS_INITIALIZING: bool = false;

thread_local! {
    static SR_WEBVIEW: std::cell::RefCell<Option<wry::WebView>> = const { std::cell::RefCell::new(None) };
    static SR_WEB_CONTEXT: std::cell::RefCell<Option<WebContext>> = const { std::cell::RefCell::new(None) };
}

lazy_static::lazy_static! {
    pub static ref SERVER_PORT: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(0);
}

static REPO_ROOT_CACHE: OnceLock<Option<PathBuf>> = OnceLock::new();

fn is_repo_root(path: &Path) -> bool {
    path.join("Cargo.toml").exists()
        && path.join("screen-record").exists()
        && path.join("src").exists()
}

fn repo_root() -> Option<PathBuf> {
    REPO_ROOT_CACHE
        .get_or_init(|| {
            let mut dir = std::env::current_dir().ok()?;
            for _ in 0..8 {
                if is_repo_root(&dir) {
                    return Some(dir);
                }
                if !dir.pop() {
                    break;
                }
            }
            None
        })
        .clone()
}

/// Serve downloaded background images from %LOCALAPPDATA%/screen-goated-toolbox/backgrounds/
/// Path format: /bg-downloaded/{filename}  e.g. /bg-downloaded/warm-abstract.png
fn try_read_downloaded_bg(path: &str) -> Option<(Vec<u8>, &'static str)> {
    let prefix = "/bg-downloaded/";
    let rel = path.strip_prefix(prefix)?;
    let rel = rel
        .split_once('?')
        .map(|(p, _)| p)
        .unwrap_or(rel)
        .split_once('#')
        .map(|(p, _)| p)
        .unwrap_or(rel);
    if rel.is_empty() || rel.contains("..") || rel.contains('/') || rel.contains('\\') {
        return None;
    }
    let dir = dirs::data_local_dir()?
        .join("screen-goated-toolbox")
        .join("backgrounds");
    let file_path = dir.join(rel);
    let bytes = std::fs::read(&file_path).ok()?;
    let mime = if rel.ends_with(".jpg") || rel.ends_with(".jpeg") {
        "image/jpeg"
    } else if rel.ends_with(".webp") {
        "image/webp"
    } else if rel.ends_with(".svg") {
        "image/svg+xml"
    } else {
        "image/png"
    };
    Some((bytes, mime))
}

fn try_read_runtime_cursor_svg(path: &str) -> Option<Vec<u8>> {
    if !path.ends_with(".svg") {
        return None;
    }

    let rel = path.trim_start_matches('/');
    if rel.is_empty() || rel.contains("..") || rel.contains('\\') {
        return None;
    }
    if !(rel.starts_with("cursor-") || rel.starts_with("cursors/")) {
        return None;
    }

    let root = repo_root()?;
    let candidates = [
        root.join("src")
            .join("overlay")
            .join("screen_record")
            .join("dist")
            .join(rel),
        root.join("screen-record").join("public").join(rel),
    ];

    for candidate in candidates {
        if let Ok(bytes) = std::fs::read(&candidate) {
            return Some(bytes);
        }
    }
    None
}

#[derive(Deserialize)]
struct IpcRequest {
    id: String,
    cmd: String,
    #[serde(default)]
    args: serde_json::Value,
}

// --- WINDOW PROCEDURE ---

unsafe extern "system" fn sr_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        match msg {
            WM_APP_SHOW => {
                let _ = ShowWindow(hwnd, SW_SHOW);
                let _ = SetForegroundWindow(hwnd);
                let _ = SetFocus(Some(hwnd));
                // Push current theme/lang on show
                push_settings_to_webview();
                LRESULT(0)
            }
            WM_APP_UPDATE_SETTINGS => {
                push_settings_to_webview();
                LRESULT(0)
            }
            WM_ERASEBKGND => {
                LRESULT(1) // Suppress — WebView covers full client area
            }
            WM_CLOSE => {
                let _ = ShowWindow(hwnd, SW_HIDE);
                LRESULT(0)
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            WM_NCCALCSIZE => {
                if wparam.0 == 1 {
                    let params = &mut *(lparam.0 as *mut NCCALCSIZE_PARAMS);
                    if IsZoomed(hwnd).as_bool() {
                        let frame_x =
                            GetSystemMetrics(SM_CXFRAME) + GetSystemMetrics(SM_CXPADDEDBORDER);
                        let frame_y =
                            GetSystemMetrics(SM_CYFRAME) + GetSystemMetrics(SM_CXPADDEDBORDER);
                        params.rgrc[0].left += frame_x;
                        params.rgrc[0].top += frame_y;
                        params.rgrc[0].right -= frame_x;
                        params.rgrc[0].bottom -= frame_y;
                    }
                }
                LRESULT(0)
            }
            WM_NCHITTEST => {
                let x = lparam.0 as i16 as i32;
                let y = (lparam.0 >> 16) as i16 as i32;

                // GetWindowRect includes the invisible DWM shadow (~7px each side).
                // Use DWMWA_EXTENDED_FRAME_BOUNDS for the actual visible rect so resize
                // zones are measured from the visible edge, not from inside the shadow.
                let mut rect = RECT::default();
                let _ = GetWindowRect(hwnd, &mut rect);
                let mut frame = rect;
                let _ = DwmGetWindowAttribute(
                    hwnd,
                    DWMWA_EXTENDED_FRAME_BOUNDS,
                    &mut frame as *mut _ as *mut std::ffi::c_void,
                    std::mem::size_of::<RECT>() as u32,
                );

                let border = 6; // px into visible area; shadow zone is always resize
                let title_height = 44;

                // Resize zones: shadow (outside visible) + `border` px inside visible
                let top = frame.top + border;
                let bottom = frame.bottom - border;
                let left = frame.left + border;
                let right = frame.right - border;

                if y < top {
                    if x < left {
                        return LRESULT(HTTOPLEFT as isize);
                    }
                    if x > right {
                        return LRESULT(HTTOPRIGHT as isize);
                    }
                    return LRESULT(HTTOP as isize);
                }
                if y > bottom {
                    if x < left {
                        return LRESULT(HTBOTTOMLEFT as isize);
                    }
                    if x > right {
                        return LRESULT(HTBOTTOMRIGHT as isize);
                    }
                    return LRESULT(HTBOTTOM as isize);
                }
                if x < left {
                    return LRESULT(HTLEFT as isize);
                }
                if x > right {
                    return LRESULT(HTRIGHT as isize);
                }

                if y < frame.top + title_height {
                    return LRESULT(HTCLIENT as isize);
                }

                LRESULT(HTCLIENT as isize)
            }
            WM_GETMINMAXINFO => {
                let info = &mut *(lparam.0 as *mut MINMAXINFO);
                info.ptMinTrackSize.x = 800;
                info.ptMinTrackSize.y = 500;
                LRESULT(0)
            }
            WM_EXITSIZEMOVE => {
                // Persist restored (non-maximized/minimized) screen-record window size.
                if !IsZoomed(hwnd).as_bool() && !IsIconic(hwnd).as_bool() {
                    let mut rect = RECT::default();
                    let _ = GetWindowRect(hwnd, &mut rect);
                    let w = (rect.right - rect.left).max(800);
                    let h = (rect.bottom - rect.top).max(500);
                    {
                        let mut app = crate::APP.lock().unwrap();
                        app.config.screen_record_window_size = (w, h);
                        crate::config::save_config(&app.config);
                    }
                }
                LRESULT(0)
            }
            WM_SIZE => {
                SR_WEBVIEW.with(|wv| {
                    if let Some(webview) = wv.borrow().as_ref() {
                        let mut r = RECT::default();
                        let _ = GetClientRect(hwnd, &mut r);
                        let w = (r.right - r.left).max(0);
                        let h = (r.bottom - r.top).max(0);
                        let _ = webview.set_bounds(Rect {
                            position: wry::dpi::Position::Physical(
                                wry::dpi::PhysicalPosition::new(0, 0),
                            ),
                            size: wry::dpi::Size::Physical(wry::dpi::PhysicalSize::new(
                                w as u32, h as u32,
                            )),
                        });
                    }
                });
                LRESULT(0)
            }
            WM_APP_TOGGLE => {
                SR_WEBVIEW.with(|wv| {
                    if let Some(webview) = wv.borrow().as_ref() {
                        let _ = webview.evaluate_script(
                            "window.dispatchEvent(new CustomEvent('toggle-recording'));",
                        );
                    }
                });
                LRESULT(0)
            }
            WM_SETFOCUS => {
                SR_WEBVIEW.with(|wv| {
                    if let Some(webview) = wv.borrow().as_ref() {
                        let _ = webview.focus();
                    }
                });
                LRESULT(0)
            }
            WM_APP_RUN_SCRIPT => {
                let script_ptr = lparam.0 as *mut String;
                if !script_ptr.is_null() {
                    let script = Box::from_raw(script_ptr);
                    SR_WEBVIEW.with(|wv| {
                        if let Some(webview) = wv.borrow().as_ref() {
                            let _ = webview.evaluate_script(&script);
                        }
                    });
                }
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

// --- HWND WRAPPER ---

struct HwndWrapper(HWND);

impl HasWindowHandle for HwndWrapper {
    fn window_handle(&self) -> std::result::Result<WindowHandle<'_>, HandleError> {
        let hwnd = self.0.0 as isize;
        if hwnd == 0 {
            return Err(HandleError::Unavailable);
        }
        if let Some(non_zero) = NonZeroIsize::new(hwnd) {
            let mut handle = Win32WindowHandle::new(non_zero);
            handle.hinstance = None;
            let raw = RawWindowHandle::Win32(handle);
            Ok(unsafe { WindowHandle::borrow_raw(raw) })
        } else {
            Err(HandleError::Unavailable)
        }
    }
}

fn wnd_http_response(
    status: u16,
    content_type: &str,
    body: Cow<'static, [u8]>,
) -> wry::http::Response<Cow<'static, [u8]>> {
    wry::http::Response::builder()
        .status(status)
        .header("Content-Type", content_type)
        .header("Access-Control-Allow-Origin", "*")
        .body(body)
        .unwrap_or_else(|_| {
            wry::http::Response::builder()
                .status(500)
                .body(Cow::Borrowed(b"Internal Error".as_slice()))
                .unwrap()
        })
}

// --- PUBLIC API ---

pub fn show_screen_record() {
    unsafe {
        if !IS_WARMED_UP {
            if !IS_INITIALIZING {
                IS_INITIALIZING = true;
                std::thread::spawn(|| {
                    internal_create_sr_loop();
                });
            }

            std::thread::spawn(|| {
                for _ in 0..100 {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    let hwnd_wrapper = std::ptr::addr_of!(SR_HWND).read();
                    if IS_WARMED_UP && !hwnd_wrapper.is_invalid() {
                        let _ =
                            PostMessageW(Some(hwnd_wrapper.0), WM_APP_SHOW, WPARAM(0), LPARAM(0));
                        return;
                    }
                }
            });
            return;
        }

        let hwnd_wrapper = std::ptr::addr_of!(SR_HWND).read();
        if !hwnd_wrapper.is_invalid() {
            let _ = PostMessageW(Some(hwnd_wrapper.0), WM_APP_SHOW, WPARAM(0), LPARAM(0));
        }
    }
}

pub fn toggle_recording() {
    unsafe {
        let hwnd_wrapper = std::ptr::addr_of!(SR_HWND).read();

        if hwnd_wrapper.is_invalid() {
            show_screen_record();
        } else if IsWindowVisible(hwnd_wrapper.0).as_bool() {
            let _ = PostMessageW(Some(hwnd_wrapper.0), WM_APP_TOGGLE, WPARAM(0), LPARAM(0));
        } else {
            show_screen_record();
        }
    }
}

pub fn update_settings() {
    unsafe {
        let hwnd = std::ptr::addr_of!(SR_HWND).read();
        if !hwnd.is_invalid() {
            let _ = PostMessageW(Some(hwnd.0), WM_APP_UPDATE_SETTINGS, WPARAM(0), LPARAM(0));
        }
    }
}

fn push_settings_to_webview() {
    let (lang, theme_mode) = {
        let app = crate::APP.lock().unwrap();
        (
            app.config.ui_language.clone(),
            app.config.theme_mode.clone(),
        )
    };

    let theme_str = match theme_mode {
        crate::config::ThemeMode::Dark => "dark",
        crate::config::ThemeMode::Light => "light",
        crate::config::ThemeMode::System => {
            if crate::gui::utils::is_system_in_dark_mode() {
                "dark"
            } else {
                "light"
            }
        }
    };

    // Update window icon based on theme
    unsafe {
        let hwnd = std::ptr::addr_of!(SR_HWND).read();
        if !hwnd.is_invalid() {
            crate::gui::utils::set_window_icon(hwnd.0, theme_str == "dark");
        }
    }

    SR_WEBVIEW.with(|wv| {
        if let Some(webview) = wv.borrow().as_ref() {
            let script = format!(
                "window.postMessage({{ type: 'sr-set-settings', theme: '{}', lang: '{}' }}, '*');",
                theme_str, lang
            );
            let _ = webview.evaluate_script(&script);
        }
    });
}

// --- WINDOW CREATION ---

unsafe fn internal_create_sr_loop() {
    unsafe {
        let instance = GetModuleHandleW(None).unwrap();
        let class_name = windows::core::w!("ScreenRecord_Class");

        REGISTER_SR_CLASS.call_once(|| {
            let wc = WNDCLASSW {
                lpfnWndProc: Some(sr_wnd_proc),
                hInstance: instance.into(),
                lpszClassName: class_name,
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap(),
                hbrBackground: CreateSolidBrush(COLORREF(0x00111111)),
                ..Default::default()
            };
            let _ = RegisterClassW(&wc);
        });

        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let screen_h = GetSystemMetrics(SM_CYSCREEN);

        let (width, height) = {
            let app = crate::APP.lock().unwrap();
            let (w, h) = app.config.screen_record_window_size;
            (w.max(1000), h.max(500))
        };
        let x = (screen_w - width) / 2;
        let y = (screen_h - height) / 2;

        let hwnd = CreateWindowExW(
            WS_EX_APPWINDOW,
            class_name,
            windows::core::w!("SGT Record"),
            WS_POPUP
                | WS_THICKFRAME
                | WS_CAPTION
                | WS_SYSMENU
                | WS_MINIMIZEBOX
                | WS_MAXIMIZEBOX
                | WS_CLIPCHILDREN,
            x,
            y,
            width,
            height,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .unwrap();

        SR_HWND = SendHwnd(hwnd);

        let corner_pref = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &corner_pref as *const _ as *const std::ffi::c_void,
            std::mem::size_of_val(&corner_pref) as u32,
        );

        let wrapper = HwndWrapper(hwnd);

        SR_WEB_CONTEXT.with(|ctx| {
            if ctx.borrow().is_none() {
                let shared_data_dir = crate::overlay::get_shared_webview_data_dir(Some("common"));
                *ctx.borrow_mut() = Some(WebContext::new(Some(shared_data_dir)));
            }
        });

        std::thread::sleep(std::time::Duration::from_millis(100));

        let font_style_tag = r#"<style id="sgt-font-face">
        @font-face {
            font-family: 'Google Sans Flex';
            src: url('/font.ttf') format('truetype');
            font-weight: 100 1000;
            font-style: normal;
            font-display: swap;
        }
    </style>"#
            .to_string();

        // Read initial theme/lang from config
        let (init_lang, init_theme_mode) = {
            let app = crate::APP.lock().unwrap();
            (
                app.config.ui_language.clone(),
                app.config.theme_mode.clone(),
            )
        };
        let init_theme = match init_theme_mode {
            crate::config::ThemeMode::Dark => "dark",
            crate::config::ThemeMode::Light => "light",
            crate::config::ThemeMode::System => {
                if crate::gui::utils::is_system_in_dark_mode() {
                    "dark"
                } else {
                    "light"
                }
            }
        };
        let webview_background_rgba = if init_theme == "dark" {
            (9, 9, 11, 255)
        } else {
            (250, 250, 250, 255)
        };
        let themed_html_root = if init_theme == "dark" {
            "<html lang=\"en\" class=\"dark\" data-sr-initial-theme=\"dark\">"
        } else {
            "<html lang=\"en\" data-sr-initial-theme=\"light\">"
        };

        // Set window icon based on initial theme
        crate::gui::utils::set_window_icon(hwnd, init_theme == "dark");

        let init_script = format!(
            r#"
        (function() {{
            const originalPostMessage = window.ipc.postMessage;
            window.isWry = true;
            window.invoke = async (cmd, args = {{}}) => {{
                return new Promise((resolve, reject) => {{
                    const id = Math.random().toString(36).substring(7);
                    const handler = (e) => {{
                        if (e.detail && e.detail.id === id) {{
                            window.removeEventListener('ipc-reply', handler);
                            if (e.detail.error) reject(e.detail.error);
                            else resolve(e.detail.result);
                        }}
                    }};
                    window.addEventListener('ipc-reply', handler);
                    originalPostMessage(JSON.stringify({{ id, cmd, args }}));
                }});
            }};
            // Set initial settings synchronously so React can read on mount
            window.__SR_INITIAL_THEME__ = '{init_theme}';
            window.__SR_INITIAL_LANG__ = '{init_lang}';
            document.title = 'SGT Record';
            if (document.documentElement) {{
                if ('{init_theme}' === 'dark') {{
                    document.documentElement.classList.add('dark');
                }} else {{
                    document.documentElement.classList.remove('dark');
                }}
            }}
        }})();
    "#
        );

        let webview_result = {
            let _init_lock = crate::overlay::GLOBAL_WEBVIEW_MUTEX.lock().unwrap();

            SR_WEB_CONTEXT.with(|ctx| {
            let mut ctx_ref = ctx.borrow_mut();
            let mut builder = WebViewBuilder::new_with_web_context(ctx_ref.as_mut().unwrap())
                .with_background_color(webview_background_rgba)
                .with_custom_protocol("screenrecord".to_string(), {
                    let font_style_tag = font_style_tag.clone();
                    let themed_html_root = themed_html_root.to_string();
                    move |_id, request| {
                    let path = request.uri().path();
                    if path == "/font.ttf" {
                        return wnd_http_response(200, "font/ttf", Cow::Borrowed(GOOGLE_SANS_FLEX));
                    }
                    if let Some(bytes) = try_read_runtime_cursor_svg(path) {
                        return wnd_http_response(200, "image/svg+xml", Cow::Owned(bytes));
                    }
                    if let Some((bytes, mime)) = try_read_downloaded_bg(path) {
                        return wnd_http_response(200, mime, Cow::Owned(bytes));
                    }
                    let (content, mime) = if path == "/" || path == "/index.html" {
                        // Inject initial theme class and font CSS into HTML <head> before React mounts.
                        let html = String::from_utf8_lossy(embedded_assets::INDEX_HTML);
                        let themed = html.replace("<html lang=\"en\">", &themed_html_root);
                        let modified = themed.replace("</head>", &format!("{font_style_tag}</head>"));
                        (Cow::Owned(modified.into_bytes()), "text/html")
                    } else if let Some((bytes, mime)) = embedded_assets::lookup_packaged_asset(path) {
                        (Cow::Borrowed(bytes), mime)
                    } else {
                        return wnd_http_response(
                            404,
                            "text/plain",
                            Cow::Borrowed(b"Not Found".as_slice()),
                        );
                    };
                    wnd_http_response(200, mime, content)
                }})
                .with_initialization_script(&init_script)
                .with_ipc_handler({
                    let send_hwnd = SendHwnd(hwnd);
                    move |msg: wry::http::Request<String>| {
                        let body = msg.body().as_str();
                        let hwnd = send_hwnd.0;
                        if body == "drag_window" {
                            let _ = ReleaseCapture();
                            let _ = SendMessageW(
                                hwnd,
                                WM_NCLBUTTONDOWN,
                                Some(WPARAM(HTCAPTION as usize)),
                                Some(LPARAM(0)),
                            );
                        } else if let Some(dir) = body.strip_prefix("resize_") {
                            let ht = match dir {
                                "n" => HTTOP as usize,
                                "s" => HTBOTTOM as usize,
                                "w" => HTLEFT as usize,
                                "e" => HTRIGHT as usize,
                                "nw" => HTTOPLEFT as usize,
                                "ne" => HTTOPRIGHT as usize,
                                "sw" => HTBOTTOMLEFT as usize,
                                "se" => HTBOTTOMRIGHT as usize,
                                _ => 0,
                            };
                            if ht != 0 {
                                let _ = ReleaseCapture();
                                let _ = SendMessageW(
                                    hwnd,
                                    WM_NCLBUTTONDOWN,
                                    Some(WPARAM(ht)),
                                    Some(LPARAM(0)),
                                );
                            }
                        } else if body == "minimize_window" {
                            let _ = ShowWindow(hwnd, SW_MINIMIZE);
                        } else if body == "toggle_maximize" {
                            if IsZoomed(hwnd).as_bool() {
                                let _ = ShowWindow(hwnd, SW_RESTORE);
                            } else {
                                let _ = ShowWindow(hwnd, SW_MAXIMIZE);
                            }
                        } else if body == "close_window" {
                            let _ = ShowWindow(hwnd, SW_HIDE);
                        } else if body == "enter_fullscreen" {
                            // Save current window rect so we can restore it on exit
                            let mut rect = RECT::default();
                            let _ = GetWindowRect(hwnd, &mut rect);
                            *PRE_FULLSCREEN_RECT.lock().unwrap() = Some((
                                rect.left, rect.top, rect.right, rect.bottom,
                            ));
                            // Expand to the full monitor rect (covers taskbar too)
                            let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
                            let mut mi = MONITORINFO {
                                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                                ..Default::default()
                            };
                            let _ = GetMonitorInfoW(monitor, &mut mi);
                            let r = mi.rcMonitor;
                            let _ = SetWindowPos(
                                hwnd,
                                Some(HWND_TOPMOST),
                                r.left, r.top,
                                r.right - r.left, r.bottom - r.top,
                                SWP_NOACTIVATE | SWP_SHOWWINDOW,
                            );
                        } else if body == "exit_fullscreen" {
                            let saved = PRE_FULLSCREEN_RECT.lock().unwrap().take();
                            if let Some((l, t, r, b)) = saved {
                                let _ = SetWindowPos(
                                    hwnd,
                                    Some(HWND_NOTOPMOST),
                                    l, t, r - l, b - t,
                                    SWP_NOACTIVATE | SWP_SHOWWINDOW,
                                );
                            } else {
                                // Fallback: just remove topmost without moving
                                let _ = SetWindowPos(
                                    hwnd,
                                    Some(HWND_NOTOPMOST),
                                    0, 0, 0, 0,
                                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                                );
                            }
                        } else if let Ok(req) = {
                            let t0 = std::time::Instant::now();
                            let r = serde_json::from_str::<IpcRequest>(body);
                            let elapsed = t0.elapsed();
                            if elapsed.as_millis() > 50 {
                                eprintln!("[IPC] Body parse: {:.0}ms ({}KB)", elapsed.as_secs_f64() * 1000.0, body.len() / 1024);
                            }
                            r
                        } {
                            let id = req.id;
                            let cmd = req.cmd;
                            let args = req.args;
                            let target_hwnd_val = send_hwnd.as_isize();

                            thread::spawn(move || {
                                let result = handle_ipc_command(cmd, args);
                                let json_res = match result {
                                    Ok(res) => serde_json::json!({ "id": id, "result": res }),
                                    Err(err) => serde_json::json!({ "id": id, "error": err }),
                                };
                                let script = format!(
                                    "window.dispatchEvent(new CustomEvent('ipc-reply', {{ detail: {} }}))",
                                    json_res
                                );

                                let script_ptr = Box::into_raw(Box::new(script));
                                let _ = PostMessageW(
                                    Some(HWND(target_hwnd_val as *mut std::ffi::c_void)),
                                    WM_APP_RUN_SCRIPT,
                                    WPARAM(0),
                                    LPARAM(script_ptr as isize),
                                );
                            });
                        }
                    }
                })
                .with_url("screenrecord://localhost/index.html");

            builder = crate::overlay::html_components::font_manager::configure_webview(builder);
            builder.build_as_child(&wrapper)
        })
        };

        let webview = match webview_result {
            Ok(wv) => wv,
            Err(e) => {
                eprintln!("Failed to create ScreenRecord WebView: {:?}", e);
                let _ = DestroyWindow(hwnd);
                SR_HWND = SendHwnd::default();
                return;
            }
        };
        let mut r = RECT::default();
        let _ = GetClientRect(hwnd, &mut r);
        let _ = webview.set_bounds(Rect {
            position: wry::dpi::Position::Physical(wry::dpi::PhysicalPosition::new(0, 0)),
            size: wry::dpi::Size::Physical(wry::dpi::PhysicalSize::new(
                (r.right - r.left).max(0) as u32,
                (r.bottom - r.top).max(0) as u32,
            )),
        });

        SR_WEBVIEW.with(|wv| {
            *wv.borrow_mut() = Some(webview);
        });

        IS_WARMED_UP = true;

        let port = ipc::start_global_media_server().unwrap_or(0);
        SERVER_PORT.store(port, std::sync::atomic::Ordering::SeqCst);

        // Prepare export GPU pipeline in the background so first export starts faster.
        thread::spawn(|| {
            native_export::warm_up_export_pipeline();
        });

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            let _ = DispatchMessageW(&msg);
        }

        SR_WEBVIEW.with(|wv| {
            *wv.borrow_mut() = None;
        });
        SR_HWND = SendHwnd::default();
        IS_WARMED_UP = false;
        IS_INITIALIZING = false;
    }
}
