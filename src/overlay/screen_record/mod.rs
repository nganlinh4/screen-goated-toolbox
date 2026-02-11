// --- SCREEN RECORD MODULE ---
// Screen recording overlay with WebView interface.

mod ffmpeg;
mod ipc;

use raw_window_handle::{
    HandleError, HasWindowHandle, RawWindowHandle, Win32WindowHandle, WindowHandle,
};
use serde::Deserialize;
use std::borrow::Cow;
use std::num::NonZeroIsize;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Once, OnceLock};
use std::thread;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::{
    DwmGetWindowAttribute, DwmSetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS,
    DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
};
use windows::Win32::Graphics::Gdi::CreateSolidBrush;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::*;
use wry::{Rect, WebContext, WebViewBuilder};

use crate::win_types::SendHwnd;

pub mod audio_engine;
pub mod engine;
pub mod gpu_export;
pub mod keyviz;
pub mod native_export;

// Re-exports
pub use ffmpeg::{get_ffmpeg_path, get_ffprobe_path};
use ipc::handle_ipc_command;

// --- CONSTANTS ---
const WM_APP_SHOW: u32 = WM_USER + 110;
const WM_APP_TOGGLE: u32 = WM_USER + 111;
const WM_APP_RUN_SCRIPT: u32 = WM_USER + 112;
const WM_APP_UPDATE_SETTINGS: u32 = WM_USER + 113;

// --- STATE ---
static REGISTER_SR_CLASS: Once = Once::new();
pub static mut SR_HWND: SendHwnd = SendHwnd(HWND(std::ptr::null_mut()));
static mut IS_WARMED_UP: bool = false;
static mut IS_INITIALIZING: bool = false;

thread_local! {
    static SR_WEBVIEW: std::cell::RefCell<Option<Arc<wry::WebView>>> = std::cell::RefCell::new(None);
    static SR_WEB_CONTEXT: std::cell::RefCell<Option<WebContext>> = std::cell::RefCell::new(None);
}

lazy_static::lazy_static! {
    pub static ref SERVER_PORT: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(0);
    static ref FFMPEG_PROCESS: Mutex<Option<std::process::Child>> = Mutex::new(None);
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

// --- ASSETS ---
const INDEX_HTML: &[u8] = include_bytes!("dist/index.html");
const ASSET_INDEX_JS: &[u8] = include_bytes!("dist/assets/index.js");
const ASSET_INDEX_CSS: &[u8] = include_bytes!("dist/assets/index.css");
const ASSET_VITE_SVG: &[u8] = include_bytes!("dist/vite.svg");
const ASSET_TAURI_SVG: &[u8] = include_bytes!("dist/tauri.svg");
const ASSET_POINTER_SVG: &[u8] = include_bytes!("dist/pointer.svg");
const ASSET_CURSOR_DEFAULT_SCREENSTUDIO_SVG: &[u8] = include_bytes!("dist/cursor-default-screenstudio.svg");
const ASSET_CURSOR_TEXT_SCREENSTUDIO_SVG: &[u8] = include_bytes!("dist/cursor-text-screenstudio.svg");
const ASSET_CURSOR_POINTER_SCREENSTUDIO_SVG: &[u8] = include_bytes!("dist/cursor-pointer-screenstudio.svg");
const ASSET_CURSOR_OPENHAND_SCREENSTUDIO_SVG: &[u8] = include_bytes!("dist/cursor-openhand-screenstudio.svg");
const ASSET_CURSOR_CLOSEHAND_SCREENSTUDIO_SVG: &[u8] = include_bytes!("dist/cursor-closehand-screenstudio.svg");
const ASSET_CURSOR_WAIT_SCREENSTUDIO_SVG: &[u8] = include_bytes!("dist/cursor-wait-screenstudio.svg");
const ASSET_CURSOR_APPSTARTING_SCREENSTUDIO_SVG: &[u8] = include_bytes!("dist/cursor-appstarting-screenstudio.svg");
const ASSET_CURSOR_CROSSHAIR_SCREENSTUDIO_SVG: &[u8] = include_bytes!("dist/cursor-crosshair-screenstudio.svg");
const ASSET_CURSOR_RESIZE_NS_SCREENSTUDIO_SVG: &[u8] = include_bytes!("dist/cursor-resize-ns-screenstudio.svg");
const ASSET_CURSOR_RESIZE_WE_SCREENSTUDIO_SVG: &[u8] = include_bytes!("dist/cursor-resize-we-screenstudio.svg");
const ASSET_CURSOR_RESIZE_NWSE_SCREENSTUDIO_SVG: &[u8] = include_bytes!("dist/cursor-resize-nwse-screenstudio.svg");
const ASSET_CURSOR_RESIZE_NESW_SCREENSTUDIO_SVG: &[u8] = include_bytes!("dist/cursor-resize-nesw-screenstudio.svg");
const ASSET_CURSOR_DEFAULT_MACOS26_SVG: &[u8] = include_bytes!("dist/cursor-default-macos26.svg");
const ASSET_CURSOR_TEXT_MACOS26_SVG: &[u8] = include_bytes!("dist/cursor-text-macos26.svg");
const ASSET_CURSOR_POINTER_MACOS26_SVG: &[u8] = include_bytes!("dist/cursor-pointer-macos26.svg");
const ASSET_CURSOR_OPENHAND_MACOS26_SVG: &[u8] = include_bytes!("dist/cursor-openhand-macos26.svg");
const ASSET_CURSOR_CLOSEHAND_MACOS26_SVG: &[u8] = include_bytes!("dist/cursor-closehand-macos26.svg");
const ASSET_CURSOR_WAIT_MACOS26_SVG: &[u8] = include_bytes!("dist/cursor-wait-macos26.svg");
const ASSET_CURSOR_APPSTARTING_MACOS26_SVG: &[u8] = include_bytes!("dist/cursor-appstarting-macos26.svg");
const ASSET_CURSOR_CROSSHAIR_MACOS26_SVG: &[u8] = include_bytes!("dist/cursor-crosshair-macos26.svg");
const ASSET_CURSOR_RESIZE_NS_MACOS26_SVG: &[u8] = include_bytes!("dist/cursor-resize-ns-macos26.svg");
const ASSET_CURSOR_RESIZE_WE_MACOS26_SVG: &[u8] = include_bytes!("dist/cursor-resize-we-macos26.svg");
const ASSET_CURSOR_RESIZE_NWSE_MACOS26_SVG: &[u8] = include_bytes!("dist/cursor-resize-nwse-macos26.svg");
const ASSET_CURSOR_RESIZE_NESW_MACOS26_SVG: &[u8] = include_bytes!("dist/cursor-resize-nesw-macos26.svg");
const ASSET_CURSOR_DEFAULT_SGTCUTE_SVG: &[u8] = include_bytes!("dist/cursor-default-sgtcute.svg");
const ASSET_CURSOR_TEXT_SGTCUTE_SVG: &[u8] = include_bytes!("dist/cursor-text-sgtcute.svg");
const ASSET_CURSOR_POINTER_SGTCUTE_SVG: &[u8] = include_bytes!("dist/cursor-pointer-sgtcute.svg");
const ASSET_CURSOR_OPENHAND_SGTCUTE_SVG: &[u8] = include_bytes!("dist/cursor-openhand-sgtcute.svg");
const ASSET_CURSOR_CLOSEHAND_SGTCUTE_SVG: &[u8] = include_bytes!("dist/cursor-closehand-sgtcute.svg");
const ASSET_CURSOR_WAIT_SGTCUTE_SVG: &[u8] = include_bytes!("dist/cursor-wait-sgtcute.svg");
const ASSET_CURSOR_APPSTARTING_SGTCUTE_SVG: &[u8] = include_bytes!("dist/cursor-appstarting-sgtcute.svg");
const ASSET_CURSOR_CROSSHAIR_SGTCUTE_SVG: &[u8] = include_bytes!("dist/cursor-crosshair-sgtcute.svg");
const ASSET_CURSOR_RESIZE_NS_SGTCUTE_SVG: &[u8] = include_bytes!("dist/cursor-resize-ns-sgtcute.svg");
const ASSET_CURSOR_RESIZE_WE_SGTCUTE_SVG: &[u8] = include_bytes!("dist/cursor-resize-we-sgtcute.svg");
const ASSET_CURSOR_RESIZE_NWSE_SGTCUTE_SVG: &[u8] = include_bytes!("dist/cursor-resize-nwse-sgtcute.svg");
const ASSET_CURSOR_RESIZE_NESW_SGTCUTE_SVG: &[u8] = include_bytes!("dist/cursor-resize-nesw-sgtcute.svg");
const ASSET_CURSOR_DEFAULT_SGTCOOL_SVG: &[u8] = include_bytes!("dist/cursor-default-sgtcool.svg");
const ASSET_CURSOR_TEXT_SGTCOOL_SVG: &[u8] = include_bytes!("dist/cursor-text-sgtcool.svg");
const ASSET_CURSOR_POINTER_SGTCOOL_SVG: &[u8] = include_bytes!("dist/cursor-pointer-sgtcool.svg");
const ASSET_CURSOR_OPENHAND_SGTCOOL_SVG: &[u8] = include_bytes!("dist/cursor-openhand-sgtcool.svg");
const ASSET_CURSOR_CLOSEHAND_SGTCOOL_SVG: &[u8] = include_bytes!("dist/cursor-closehand-sgtcool.svg");
const ASSET_CURSOR_WAIT_SGTCOOL_SVG: &[u8] = include_bytes!("dist/cursor-wait-sgtcool.svg");
const ASSET_CURSOR_APPSTARTING_SGTCOOL_SVG: &[u8] = include_bytes!("dist/cursor-appstarting-sgtcool.svg");
const ASSET_CURSOR_CROSSHAIR_SGTCOOL_SVG: &[u8] = include_bytes!("dist/cursor-crosshair-sgtcool.svg");
const ASSET_CURSOR_RESIZE_NS_SGTCOOL_SVG: &[u8] = include_bytes!("dist/cursor-resize-ns-sgtcool.svg");
const ASSET_CURSOR_RESIZE_WE_SGTCOOL_SVG: &[u8] = include_bytes!("dist/cursor-resize-we-sgtcool.svg");
const ASSET_CURSOR_RESIZE_NWSE_SGTCOOL_SVG: &[u8] = include_bytes!("dist/cursor-resize-nwse-sgtcool.svg");
const ASSET_CURSOR_RESIZE_NESW_SGTCOOL_SVG: &[u8] = include_bytes!("dist/cursor-resize-nesw-sgtcool.svg");
const ASSET_CURSOR_DEFAULT_SGTAI_SVG: &[u8] = include_bytes!("dist/cursor-default-sgtai.svg");
const ASSET_CURSOR_TEXT_SGTAI_SVG: &[u8] = include_bytes!("dist/cursor-text-sgtai.svg");
const ASSET_CURSOR_POINTER_SGTAI_SVG: &[u8] = include_bytes!("dist/cursor-pointer-sgtai.svg");
const ASSET_CURSOR_OPENHAND_SGTAI_SVG: &[u8] = include_bytes!("dist/cursor-openhand-sgtai.svg");
const ASSET_CURSOR_CLOSEHAND_SGTAI_SVG: &[u8] = include_bytes!("dist/cursor-closehand-sgtai.svg");
const ASSET_CURSOR_WAIT_SGTAI_SVG: &[u8] = include_bytes!("dist/cursor-wait-sgtai.svg");
const ASSET_CURSOR_APPSTARTING_SGTAI_SVG: &[u8] = include_bytes!("dist/cursor-appstarting-sgtai.svg");
const ASSET_CURSOR_CROSSHAIR_SGTAI_SVG: &[u8] = include_bytes!("dist/cursor-crosshair-sgtai.svg");
const ASSET_CURSOR_RESIZE_NS_SGTAI_SVG: &[u8] = include_bytes!("dist/cursor-resize-ns-sgtai.svg");
const ASSET_CURSOR_RESIZE_WE_SGTAI_SVG: &[u8] = include_bytes!("dist/cursor-resize-we-sgtai.svg");
const ASSET_CURSOR_RESIZE_NWSE_SGTAI_SVG: &[u8] = include_bytes!("dist/cursor-resize-nwse-sgtai.svg");
const ASSET_CURSOR_RESIZE_NESW_SGTAI_SVG: &[u8] = include_bytes!("dist/cursor-resize-nesw-sgtai.svg");
const ASSET_CURSOR_DEFAULT_SGTPIXEL_SVG: &[u8] = include_bytes!("dist/cursor-default-sgtpixel.svg");
const ASSET_CURSOR_TEXT_SGTPIXEL_SVG: &[u8] = include_bytes!("dist/cursor-text-sgtpixel.svg");
const ASSET_CURSOR_POINTER_SGTPIXEL_SVG: &[u8] = include_bytes!("dist/cursor-pointer-sgtpixel.svg");
const ASSET_CURSOR_OPENHAND_SGTPIXEL_SVG: &[u8] = include_bytes!("dist/cursor-openhand-sgtpixel.svg");
const ASSET_CURSOR_CLOSEHAND_SGTPIXEL_SVG: &[u8] = include_bytes!("dist/cursor-closehand-sgtpixel.svg");
const ASSET_CURSOR_WAIT_SGTPIXEL_SVG: &[u8] = include_bytes!("dist/cursor-wait-sgtpixel.svg");
const ASSET_CURSOR_APPSTARTING_SGTPIXEL_SVG: &[u8] = include_bytes!("dist/cursor-appstarting-sgtpixel.svg");
const ASSET_CURSOR_CROSSHAIR_SGTPIXEL_SVG: &[u8] = include_bytes!("dist/cursor-crosshair-sgtpixel.svg");
const ASSET_CURSOR_RESIZE_NS_SGTPIXEL_SVG: &[u8] = include_bytes!("dist/cursor-resize-ns-sgtpixel.svg");
const ASSET_CURSOR_RESIZE_WE_SGTPIXEL_SVG: &[u8] = include_bytes!("dist/cursor-resize-we-sgtpixel.svg");
const ASSET_CURSOR_RESIZE_NWSE_SGTPIXEL_SVG: &[u8] = include_bytes!("dist/cursor-resize-nwse-sgtpixel.svg");
const ASSET_CURSOR_RESIZE_NESW_SGTPIXEL_SVG: &[u8] = include_bytes!("dist/cursor-resize-nesw-sgtpixel.svg");
const ASSET_CURSOR_SGTCOOL_SLOT_01_SVG: &[u8] = include_bytes!("dist/cursors/sgtcool_raw/slot-01.svg");
const ASSET_CURSOR_SGTCOOL_SLOT_02_SVG: &[u8] = include_bytes!("dist/cursors/sgtcool_raw/slot-02.svg");
const ASSET_CURSOR_SGTCOOL_SLOT_03_SVG: &[u8] = include_bytes!("dist/cursors/sgtcool_raw/slot-03.svg");
const ASSET_CURSOR_SGTCOOL_SLOT_04_SVG: &[u8] = include_bytes!("dist/cursors/sgtcool_raw/slot-04.svg");
const ASSET_CURSOR_SGTCOOL_SLOT_05_SVG: &[u8] = include_bytes!("dist/cursors/sgtcool_raw/slot-05.svg");
const ASSET_CURSOR_SGTCOOL_SLOT_06_SVG: &[u8] = include_bytes!("dist/cursors/sgtcool_raw/slot-06.svg");
const ASSET_CURSOR_SGTCOOL_SLOT_07_SVG: &[u8] = include_bytes!("dist/cursors/sgtcool_raw/slot-07.svg");
const ASSET_CURSOR_SGTCOOL_SLOT_08_SVG: &[u8] = include_bytes!("dist/cursors/sgtcool_raw/slot-08.svg");
const ASSET_CURSOR_SGTCOOL_SLOT_09_SVG: &[u8] = include_bytes!("dist/cursors/sgtcool_raw/slot-09.svg");
const ASSET_CURSOR_SGTCOOL_SLOT_10_SVG: &[u8] = include_bytes!("dist/cursors/sgtcool_raw/slot-10.svg");
const ASSET_CURSOR_SGTCOOL_SLOT_11_SVG: &[u8] = include_bytes!("dist/cursors/sgtcool_raw/slot-11.svg");
const ASSET_CURSOR_SGTCOOL_SLOT_12_SVG: &[u8] = include_bytes!("dist/cursors/sgtcool_raw/slot-12.svg");
const ASSET_SCREENSHOT_PNG: &[u8] = include_bytes!("dist/screenshot.png");

// --- WINDOW PROCEDURE ---

unsafe extern "system" fn sr_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
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
            return LRESULT(1); // Suppress — WebView covers full client area
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
                    let frame_x = GetSystemMetrics(SM_CXFRAME) + GetSystemMetrics(SM_CXPADDEDBORDER);
                    let frame_y = GetSystemMetrics(SM_CYFRAME) + GetSystemMetrics(SM_CXPADDEDBORDER);
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
                        position: wry::dpi::Position::Physical(wry::dpi::PhysicalPosition::new(
                            0, 0,
                        )),
                        size: wry::dpi::Size::Physical(wry::dpi::PhysicalSize::new(
                            w as u32,
                            h as u32,
                        )),
                    });
                }
            });
            LRESULT(0)
        }
        WM_APP_TOGGLE => {
            SR_WEBVIEW.with(|wv| {
                if let Some(webview) = wv.borrow().as_ref() {
                    let _ = webview
                        .evaluate_script("window.dispatchEvent(new CustomEvent('toggle-recording'));");
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
                let script = unsafe { Box::from_raw(script_ptr) };
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

// --- HWND WRAPPER ---

struct HwndWrapper(HWND);

impl HasWindowHandle for HwndWrapper {
    fn window_handle(&self) -> std::result::Result<WindowHandle<'_>, HandleError> {
        let hwnd = self.0 .0 as isize;
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
        } else {
            if IsWindowVisible(hwnd_wrapper.0).as_bool() {
                let _ = PostMessageW(Some(hwnd_wrapper.0), WM_APP_TOGGLE, WPARAM(0), LPARAM(0));
            } else {
                show_screen_record();
            }
        }
    }
}

pub fn update_settings() {
    unsafe {
        let hwnd = std::ptr::addr_of!(SR_HWND).read();
        if !hwnd.is_invalid() {
            let _ = PostMessageW(
                Some(hwnd.0),
                WM_APP_UPDATE_SETTINGS,
                WPARAM(0),
                LPARAM(0),
            );
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
    let instance = GetModuleHandleW(None).unwrap();
    let class_name = windows::core::w!("ScreenRecord_Class");

    REGISTER_SR_CLASS.call_once(|| {
        let mut wc = WNDCLASSW::default();
        wc.lpfnWndProc = Some(sr_wnd_proc);
        wc.hInstance = instance.into();
        wc.lpszClassName = class_name;
        wc.hCursor = LoadCursorW(None, IDC_ARROW).unwrap();
        wc.hbrBackground = unsafe { CreateSolidBrush(COLORREF(0x00111111)) };
        let _ = RegisterClassW(&wc);
    });

    let screen_w = GetSystemMetrics(SM_CXSCREEN);
    let screen_h = GetSystemMetrics(SM_CYSCREEN);

    let (width, height) = {
        let app = crate::APP.lock().unwrap();
        let (w, h) = app.config.screen_record_window_size;
        (w.max(800), h.max(500))
    };
    let x = (screen_w - width) / 2;
    let y = (screen_h - height) / 2;

    let hwnd = CreateWindowExW(
        WS_EX_APPWINDOW,
        class_name,
        windows::core::w!("Screen Record"),
        WS_POPUP | WS_THICKFRAME | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_MAXIMIZEBOX | WS_CLIPCHILDREN,
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

    // Font CSS from local HTTP server — CSS @font-face url() only works over http/https, not custom protocols
    let font_css = crate::overlay::html_components::font_manager::get_font_css();
    let font_style_tag = format!("<style>{}</style>", font_css);

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

    // Set window icon based on initial theme
    crate::gui::utils::set_window_icon(hwnd, init_theme == "dark");

    let init_script = format!(
        r#"
        (function() {{
            const originalPostMessage = window.ipc.postMessage;
            window.__TAURI_INTERNALS__ = {{
                invoke: async (cmd, args) => {{
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
                }}
            }};
            window.__TAURI__ = {{
                core: {{
                    invoke: window.__TAURI_INTERNALS__.invoke
                }}
            }};
            // Set initial settings synchronously so React can read on mount
            window.__SR_INITIAL_THEME__ = '{init_theme}';
            window.__SR_INITIAL_LANG__ = '{init_lang}';
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
                .with_custom_protocol("screenrecord".to_string(), {
                    let font_style_tag = font_style_tag.clone();
                    move |_id, request| {
                    let path = request.uri().path();
                    if let Some(bytes) = try_read_runtime_cursor_svg(path) {
                        return wnd_http_response(200, "image/svg+xml", Cow::Owned(bytes));
                    }
                    let (content, mime) = if path == "/" || path == "/index.html" {
                        // Inject font CSS into HTML <head> for instant font rendering
                        let html = String::from_utf8_lossy(INDEX_HTML);
                        let modified = html.replace("</head>", &format!("{font_style_tag}</head>"));
                        (Cow::Owned(modified.into_bytes()), "text/html")
                    } else if path.ends_with("index.js") {
                        (Cow::Borrowed(ASSET_INDEX_JS), "application/javascript")
                    } else if path.ends_with("index.css") {
                        (Cow::Borrowed(ASSET_INDEX_CSS), "text/css")
                    } else if path.ends_with("vite.svg") {
                        (Cow::Borrowed(ASSET_VITE_SVG), "image/svg+xml")
                    } else if path.ends_with("tauri.svg") {
                        (Cow::Borrowed(ASSET_TAURI_SVG), "image/svg+xml")
                    } else if path.ends_with("pointer.svg") {
                        (Cow::Borrowed(ASSET_POINTER_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-default-screenstudio.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_DEFAULT_SCREENSTUDIO_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-text-screenstudio.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_TEXT_SCREENSTUDIO_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-pointer-screenstudio.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_POINTER_SCREENSTUDIO_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-openhand-screenstudio.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_OPENHAND_SCREENSTUDIO_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-closehand-screenstudio.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_CLOSEHAND_SCREENSTUDIO_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-wait-screenstudio.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_WAIT_SCREENSTUDIO_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-appstarting-screenstudio.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_APPSTARTING_SCREENSTUDIO_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-crosshair-screenstudio.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_CROSSHAIR_SCREENSTUDIO_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-resize-ns-screenstudio.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_RESIZE_NS_SCREENSTUDIO_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-resize-we-screenstudio.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_RESIZE_WE_SCREENSTUDIO_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-resize-nwse-screenstudio.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_RESIZE_NWSE_SCREENSTUDIO_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-resize-nesw-screenstudio.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_RESIZE_NESW_SCREENSTUDIO_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-default-macos26.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_DEFAULT_MACOS26_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-text-macos26.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_TEXT_MACOS26_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-pointer-macos26.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_POINTER_MACOS26_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-openhand-macos26.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_OPENHAND_MACOS26_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-closehand-macos26.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_CLOSEHAND_MACOS26_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-wait-macos26.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_WAIT_MACOS26_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-appstarting-macos26.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_APPSTARTING_MACOS26_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-crosshair-macos26.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_CROSSHAIR_MACOS26_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-resize-ns-macos26.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_RESIZE_NS_MACOS26_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-resize-we-macos26.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_RESIZE_WE_MACOS26_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-resize-nwse-macos26.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_RESIZE_NWSE_MACOS26_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-resize-nesw-macos26.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_RESIZE_NESW_MACOS26_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-default-sgtcute.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_DEFAULT_SGTCUTE_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-text-sgtcute.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_TEXT_SGTCUTE_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-pointer-sgtcute.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_POINTER_SGTCUTE_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-openhand-sgtcute.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_OPENHAND_SGTCUTE_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-closehand-sgtcute.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_CLOSEHAND_SGTCUTE_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-wait-sgtcute.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_WAIT_SGTCUTE_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-appstarting-sgtcute.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_APPSTARTING_SGTCUTE_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-crosshair-sgtcute.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_CROSSHAIR_SGTCUTE_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-resize-ns-sgtcute.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_RESIZE_NS_SGTCUTE_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-resize-we-sgtcute.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_RESIZE_WE_SGTCUTE_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-resize-nwse-sgtcute.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_RESIZE_NWSE_SGTCUTE_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-resize-nesw-sgtcute.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_RESIZE_NESW_SGTCUTE_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-default-sgtcool.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_DEFAULT_SGTCOOL_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-text-sgtcool.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_TEXT_SGTCOOL_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-pointer-sgtcool.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_POINTER_SGTCOOL_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-openhand-sgtcool.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_OPENHAND_SGTCOOL_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-closehand-sgtcool.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_CLOSEHAND_SGTCOOL_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-wait-sgtcool.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_WAIT_SGTCOOL_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-appstarting-sgtcool.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_APPSTARTING_SGTCOOL_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-crosshair-sgtcool.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_CROSSHAIR_SGTCOOL_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-resize-ns-sgtcool.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_RESIZE_NS_SGTCOOL_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-resize-we-sgtcool.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_RESIZE_WE_SGTCOOL_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-resize-nwse-sgtcool.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_RESIZE_NWSE_SGTCOOL_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-resize-nesw-sgtcool.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_RESIZE_NESW_SGTCOOL_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-default-sgtai.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_DEFAULT_SGTAI_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-text-sgtai.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_TEXT_SGTAI_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-pointer-sgtai.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_POINTER_SGTAI_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-openhand-sgtai.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_OPENHAND_SGTAI_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-closehand-sgtai.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_CLOSEHAND_SGTAI_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-wait-sgtai.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_WAIT_SGTAI_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-appstarting-sgtai.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_APPSTARTING_SGTAI_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-crosshair-sgtai.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_CROSSHAIR_SGTAI_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-resize-ns-sgtai.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_RESIZE_NS_SGTAI_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-resize-we-sgtai.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_RESIZE_WE_SGTAI_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-resize-nwse-sgtai.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_RESIZE_NWSE_SGTAI_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-resize-nesw-sgtai.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_RESIZE_NESW_SGTAI_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-default-sgtpixel.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_DEFAULT_SGTPIXEL_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-text-sgtpixel.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_TEXT_SGTPIXEL_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-pointer-sgtpixel.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_POINTER_SGTPIXEL_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-openhand-sgtpixel.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_OPENHAND_SGTPIXEL_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-closehand-sgtpixel.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_CLOSEHAND_SGTPIXEL_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-wait-sgtpixel.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_WAIT_SGTPIXEL_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-appstarting-sgtpixel.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_APPSTARTING_SGTPIXEL_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-crosshair-sgtpixel.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_CROSSHAIR_SGTPIXEL_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-resize-ns-sgtpixel.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_RESIZE_NS_SGTPIXEL_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-resize-we-sgtpixel.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_RESIZE_WE_SGTPIXEL_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-resize-nwse-sgtpixel.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_RESIZE_NWSE_SGTPIXEL_SVG), "image/svg+xml")
                    } else if path.ends_with("cursor-resize-nesw-sgtpixel.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_RESIZE_NESW_SGTPIXEL_SVG), "image/svg+xml")
                    } else if path.ends_with("cursors/sgtcool_raw/slot-01.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_SGTCOOL_SLOT_01_SVG), "image/svg+xml")
                    } else if path.ends_with("cursors/sgtcool_raw/slot-02.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_SGTCOOL_SLOT_02_SVG), "image/svg+xml")
                    } else if path.ends_with("cursors/sgtcool_raw/slot-03.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_SGTCOOL_SLOT_03_SVG), "image/svg+xml")
                    } else if path.ends_with("cursors/sgtcool_raw/slot-04.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_SGTCOOL_SLOT_04_SVG), "image/svg+xml")
                    } else if path.ends_with("cursors/sgtcool_raw/slot-05.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_SGTCOOL_SLOT_05_SVG), "image/svg+xml")
                    } else if path.ends_with("cursors/sgtcool_raw/slot-06.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_SGTCOOL_SLOT_06_SVG), "image/svg+xml")
                    } else if path.ends_with("cursors/sgtcool_raw/slot-07.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_SGTCOOL_SLOT_07_SVG), "image/svg+xml")
                    } else if path.ends_with("cursors/sgtcool_raw/slot-08.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_SGTCOOL_SLOT_08_SVG), "image/svg+xml")
                    } else if path.ends_with("cursors/sgtcool_raw/slot-09.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_SGTCOOL_SLOT_09_SVG), "image/svg+xml")
                    } else if path.ends_with("cursors/sgtcool_raw/slot-10.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_SGTCOOL_SLOT_10_SVG), "image/svg+xml")
                    } else if path.ends_with("cursors/sgtcool_raw/slot-11.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_SGTCOOL_SLOT_11_SVG), "image/svg+xml")
                    } else if path.ends_with("cursors/sgtcool_raw/slot-12.svg") {
                        (Cow::Borrowed(ASSET_CURSOR_SGTCOOL_SLOT_12_SVG), "image/svg+xml")
                    } else if path.ends_with("screenshot.png") {
                        (Cow::Borrowed(ASSET_SCREENSHOT_PNG), "image/png")
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
                            if unsafe { IsZoomed(hwnd).as_bool() } {
                                let _ = ShowWindow(hwnd, SW_RESTORE);
                            } else {
                                let _ = ShowWindow(hwnd, SW_MAXIMIZE);
                            }
                        } else if body == "close_window" {
                            let _ = ShowWindow(hwnd, SW_HIDE);
                        } else if let Ok(req) = serde_json::from_str::<IpcRequest>(body) {
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
                                    json_res.to_string()
                                );

                                let script_ptr = Box::into_raw(Box::new(script));
                                unsafe {
                                    let _ = PostMessageW(
                                        Some(HWND(target_hwnd_val as *mut std::ffi::c_void)),
                                        WM_APP_RUN_SCRIPT,
                                        WPARAM(0),
                                        LPARAM(script_ptr as isize),
                                    );
                                }
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
    let webview_arc = Arc::new(webview);

    let mut r = RECT::default();
    let _ = GetClientRect(hwnd, &mut r);
    let _ = webview_arc.set_bounds(Rect {
        position: wry::dpi::Position::Physical(wry::dpi::PhysicalPosition::new(0, 0)),
        size: wry::dpi::Size::Physical(wry::dpi::PhysicalSize::new(
            (r.right - r.left).max(0) as u32,
            (r.bottom - r.top).max(0) as u32,
        )),
    });

    SR_WEBVIEW.with(|wv| {
        *wv.borrow_mut() = Some(webview_arc);
    });

    unsafe {
        IS_WARMED_UP = true;
    }

    // Prepare export GPU pipeline in the background so first export starts faster.
    thread::spawn(|| {
        native_export::warm_up_export_pipeline();
    });

    let mut msg = MSG::default();
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            let _ = DispatchMessageW(&msg);
        }
    }

    SR_WEBVIEW.with(|wv| {
        *wv.borrow_mut() = None;
    });
    unsafe {
        SR_HWND = SendHwnd::default();
        IS_WARMED_UP = false;
        IS_INITIALIZING = false;
    }
}
