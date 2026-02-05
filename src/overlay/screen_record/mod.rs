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
use std::sync::{Arc, Mutex, Once};
use std::thread;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
};
use windows::Win32::Graphics::Gdi::HBRUSH;
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

#[derive(Deserialize)]
struct IpcRequest {
    id: String,
    cmd: String,
    args: serde_json::Value,
}

// --- ASSETS ---
const INDEX_HTML: &[u8] = include_bytes!("dist/index.html");
const ASSET_INDEX_JS: &[u8] = include_bytes!("dist/assets/index.js");
const ASSET_INDEX_CSS: &[u8] = include_bytes!("dist/assets/index.css");
const ASSET_VITE_SVG: &[u8] = include_bytes!("dist/vite.svg");
const ASSET_TAURI_SVG: &[u8] = include_bytes!("dist/tauri.svg");
const ASSET_POINTER_SVG: &[u8] = include_bytes!("dist/pointer.svg");
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
            LRESULT(0)
        }
        WM_CLOSE => {
            let _ = ShowWindow(hwnd, SW_HIDE);
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_NCCALCSIZE => LRESULT(0),
        WM_NCHITTEST => {
            let mut rect = RECT::default();
            let _ = GetWindowRect(hwnd, &mut rect);
            let x = (lparam.0 & 0xFFFF) as i32;
            let y = ((lparam.0 >> 16) & 0xFFFF) as i32;

            let border = 8;
            let title_height = 44;

            if y < rect.top + border {
                if x < rect.left + border {
                    return LRESULT(HTTOPLEFT as isize);
                }
                if x > rect.right - border {
                    return LRESULT(HTTOPRIGHT as isize);
                }
                return LRESULT(HTTOP as isize);
            }
            if y > rect.bottom - border {
                if x < rect.left + border {
                    return LRESULT(HTBOTTOMLEFT as isize);
                }
                if x > rect.right - border {
                    return LRESULT(HTBOTTOMRIGHT as isize);
                }
                return LRESULT(HTBOTTOM as isize);
            }
            if x < rect.left + border {
                return LRESULT(HTLEFT as isize);
            }
            if x > rect.right - border {
                return LRESULT(HTRIGHT as isize);
            }

            if y < rect.top + title_height {
                return LRESULT(HTCLIENT as isize);
            }

            LRESULT(HTCLIENT as isize)
        }
        WM_SIZE => {
            SR_WEBVIEW.with(|wv| {
                if let Some(webview) = wv.borrow().as_ref() {
                    let mut r = RECT::default();
                    let _ = GetClientRect(hwnd, &mut r);
                    let width = r.right - r.left;
                    let height = r.bottom - r.top;
                    let _ = webview.set_bounds(Rect {
                        position: wry::dpi::Position::Physical(wry::dpi::PhysicalPosition::new(
                            0, 0,
                        )),
                        size: wry::dpi::Size::Physical(wry::dpi::PhysicalSize::new(
                            width as u32,
                            height as u32,
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
        wc.hbrBackground = HBRUSH(std::ptr::null_mut());
        let _ = RegisterClassW(&wc);
    });

    let screen_w = GetSystemMetrics(SM_CXSCREEN);
    let screen_h = GetSystemMetrics(SM_CYSCREEN);

    let width = 1300;
    let height = 850;
    let x = (screen_w - width) / 2;
    let y = (screen_h - height) / 2;

    let hwnd = CreateWindowExW(
        WS_EX_APPWINDOW,
        class_name,
        windows::core::w!("Screen Record"),
        WS_POPUP | WS_THICKFRAME | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_MAXIMIZEBOX,
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

    let webview_result = {
        let _init_lock = crate::overlay::GLOBAL_WEBVIEW_MUTEX.lock().unwrap();

        SR_WEB_CONTEXT.with(|ctx| {
            let mut ctx_ref = ctx.borrow_mut();
            let mut builder = WebViewBuilder::new_with_web_context(ctx_ref.as_mut().unwrap())
                .with_custom_protocol("screenrecord".to_string(), move |_id, request| {
                    let path = request.uri().path();
                    let (content, mime) = if path == "/" || path == "/index.html" {
                        (Cow::Borrowed(INDEX_HTML), "text/html")
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
                })
                .with_initialization_script(
                    r#"
                    (function() {
                        const originalPostMessage = window.ipc.postMessage;
                        window.__TAURI_INTERNALS__ = {
                            invoke: async (cmd, args) => {
                                return new Promise((resolve, reject) => {
                                    const id = Math.random().toString(36).substring(7);
                                    const handler = (e) => {
                                        if (e.detail && e.detail.id === id) {
                                            window.removeEventListener('ipc-reply', handler);
                                            if (e.detail.error) reject(e.detail.error);
                                            else resolve(e.detail.result);
                                        }
                                    };
                                    window.addEventListener('ipc-reply', handler);
                                    originalPostMessage(JSON.stringify({ id, cmd, args }));
                                });
                            }
                        };
                        window.__TAURI__ = {
                            core: {
                                invoke: window.__TAURI_INTERNALS__.invoke
                            }
                        };
                    })();
                "#,
                )
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
            (r.right - r.left) as u32,
            (r.bottom - r.top) as u32,
        )),
    });

    SR_WEBVIEW.with(|wv| {
        *wv.borrow_mut() = Some(webview_arc);
    });

    unsafe {
        IS_WARMED_UP = true;
    }

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
