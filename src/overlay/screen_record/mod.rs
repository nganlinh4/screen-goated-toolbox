// --- SCREEN RECORD MODULE ---
// Screen recording overlay with WebView interface.

pub(crate) mod bg_download;
mod embedded_assets;
mod ipc;
mod raw_video;
mod webview_setup;
mod window_proc;

use raw_window_handle::{
    HandleError, HasWindowHandle, RawWindowHandle, Win32WindowHandle, WindowHandle,
};
use serde::Deserialize;
use std::borrow::Cow;
use std::num::NonZeroIsize;
use std::path::{Path, PathBuf};
use std::sync::{Once, OnceLock};
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use wry::WebContext;

use crate::win_types::SendHwnd;

pub mod audio_engine;
pub mod audio_source_selection;
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
mod webcam_capture;
pub mod window_selection;

// Re-exports
pub(crate) use ipc::capture_window_thumbnail;
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
                    webview_setup::internal_create_sr_loop();
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
