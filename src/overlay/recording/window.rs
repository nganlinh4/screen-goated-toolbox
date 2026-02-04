// --- RECORDING WINDOW ---
// Window creation, WebView setup, and audio thread management.

use super::messages::recording_wnd_proc;
use super::state::*;
use super::ui::generate_html;
use crate::APP;
use std::sync::atomic::Ordering;
use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::{
    DwmExtendFrameIntoClientArea, DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE,
};
use windows::Win32::System::Com::{CoInitialize, CoUninitialize};
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use wry::{Rect, WebContext, WebViewBuilder};

/// HWND wrapper for raw_window_handle trait
pub struct HwndWrapper(pub HWND);
unsafe impl Send for HwndWrapper {}
unsafe impl Sync for HwndWrapper {}

impl raw_window_handle::HasWindowHandle for HwndWrapper {
    fn window_handle(
        &self,
    ) -> std::result::Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError>
    {
        let raw = raw_window_handle::Win32WindowHandle::new(
            std::num::NonZeroIsize::new(self.0 .0 as isize).expect("HWND cannot be null"),
        );
        let handle = raw_window_handle::RawWindowHandle::Win32(raw);
        unsafe { Ok(raw_window_handle::WindowHandle::borrow_raw(handle)) }
    }
}

pub fn internal_create_recording_window() {
    unsafe {
        let coinit = CoInitialize(None);
        crate::log_info!("[Recording] Loop Start - CoInit: {:?}", coinit);
        let instance = GetModuleHandleW(None).unwrap();
        let class_name = w!("SGT_Recording_Persistent");

        REGISTER_RECORDING_CLASS.call_once(|| {
            let mut wc = WNDCLASSW::default();
            wc.lpfnWndProc = Some(recording_wnd_proc);
            wc.hInstance = instance.into();
            wc.hCursor = LoadCursorW(None, IDC_ARROW).unwrap();
            wc.lpszClassName = class_name;
            wc.style = CS_HREDRAW | CS_VREDRAW;
            RegisterClassW(&wc);
        });

        let (ui_width, ui_height) = get_ui_dimensions();

        // Create window OFF-SCREEN initially
        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class_name,
            w!("SGT Recording Web"),
            WS_POPUP | WS_VISIBLE,
            -4000,
            -4000,
            ui_width,
            ui_height,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .unwrap();
        crate::log_info!("[Recording] Window created with HWND: {:?}", hwnd);

        RECORDING_HWND_VAL.store(hwnd.0 as isize, Ordering::SeqCst);

        // Windows 11 Rounded Corners - Disable native rounding
        let corner_pref = 1u32; // DWMWCP_DONOTROUND
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            std::ptr::addr_of!(corner_pref) as *const _,
            std::mem::size_of_val(&corner_pref) as u32,
        );

        // Glass Frame Extension (critical for per-pixel alpha with WebView)
        let margins = MARGINS {
            cxLeftWidth: -1,
            cxRightWidth: -1,
            cyTopHeight: -1,
            cyBottomHeight: -1,
        };
        let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);

        // --- WEBVIEW CREATION ---
        let wrapper = HwndWrapper(hwnd);
        let html = generate_html();

        RECORDING_WEB_CONTEXT.with(|ctx| {
            if ctx.borrow().is_none() {
                let shared_data_dir = crate::overlay::get_shared_webview_data_dir(Some("common"));
                *ctx.borrow_mut() = Some(WebContext::new(Some(shared_data_dir)));
            }
        });

        let ipc_hwnd_val = hwnd.0 as usize;
        let webview_res = {
            let _init_lock = crate::overlay::GLOBAL_WEBVIEW_MUTEX.lock().unwrap();
            crate::log_info!("[Recording] Acquired init lock. Building...");

            let build_res = RECORDING_WEB_CONTEXT.with(|ctx| {
                let mut ctx_ref = ctx.borrow_mut();
                let mut builder = if let Some(web_ctx) = ctx_ref.as_mut() {
                    WebViewBuilder::new_with_web_context(web_ctx)
                } else {
                    WebViewBuilder::new()
                };

                builder = crate::overlay::html_components::font_manager::configure_webview(builder);

                let page_url =
                    crate::overlay::html_components::font_manager::store_html_page(html.clone())
                        .unwrap_or_else(|| {
                            format!("data:text/html,{}", urlencoding::encode(&html))
                        });

                let (ui_width, ui_height) = get_ui_dimensions();
                builder
                    .with_bounds(Rect {
                        position: wry::dpi::Position::Logical(wry::dpi::LogicalPosition::new(
                            0.0, 0.0,
                        )),
                        size: wry::dpi::Size::Physical(wry::dpi::PhysicalSize::new(
                            ui_width as u32,
                            ui_height as u32,
                        )),
                    })
                    .with_transparent(true)
                    .with_background_color((0, 0, 0, 0))
                    .with_url(&page_url)
                    .with_ipc_handler(move |msg: wry::http::Request<String>| {
                        handle_ipc_message(ipc_hwnd_val, msg.body());
                    })
                    .build(&wrapper)
            });
            crate::log_info!(
                "[Recording] Build finished. Status: {}",
                if build_res.is_ok() { "OK" } else { "ERR" }
            );
            build_res
        };

        if let Ok(wv) = webview_res {
            crate::log_info!("[Recording] WebView success for HWND: {:?}", hwnd);
            RECORDING_WEBVIEW.with(|cell| *cell.borrow_mut() = Some(wv));

            // Setup Global Key Hook for ESC
            let hook = SetWindowsHookExW(
                WH_KEYBOARD_LL,
                Some(super::messages::recording_hook_proc),
                Some(GetModuleHandleW(None).unwrap().into()),
                0,
            );

            // Message Loop
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                let _ = DispatchMessageW(&msg);
            }

            if let Ok(h) = hook {
                let _ = UnhookWindowsHookEx(h);
            }
        }

        // Cleanup on FULL EXIT
        RECORDING_WEBVIEW.with(|cell| *cell.borrow_mut() = None);
        RECORDING_STATE.store(0, Ordering::SeqCst);

        let _ = CoUninitialize();
    }
}

/// Handle IPC messages from WebView
fn handle_ipc_message(hwnd_val: usize, body: &str) {
    let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
    match body {
        "pause_toggle" => {
            let paused = AUDIO_PAUSE_SIGNAL.load(Ordering::SeqCst);
            AUDIO_PAUSE_SIGNAL.store(!paused, Ordering::SeqCst);
        }
        "cancel" | "close" => {
            AUDIO_ABORT_SIGNAL.store(true, Ordering::SeqCst);
            AUDIO_STOP_SIGNAL.store(true, Ordering::SeqCst);
            unsafe {
                let _ = PostMessageW(Some(hwnd), WM_APP_HIDE, WPARAM(0), LPARAM(0));
            }
        }
        "ready" => {
            // Handshake: WebView is ready, so now we can REAL_SHOW
            unsafe {
                let _ = KillTimer(Some(hwnd), 99);
                if !CURRENT_RECORDING_HIDDEN.load(Ordering::SeqCst) {
                    let _ = SetTimer(Some(hwnd), 2, 20, None);
                }
            }
        }
        "drag_window" => unsafe {
            let _ = ReleaseCapture();
            let _ = PostMessageW(
                Some(hwnd),
                WM_NCLBUTTONDOWN,
                WPARAM(2), // HTCAPTION = 2
                LPARAM(0),
            );
        },
        _ => {}
    }
}

pub fn start_audio_thread(hwnd: HWND, preset_idx: usize) {
    let (preset, last_active_window) = {
        let app = APP.lock().unwrap();
        (
            app.config.presets[preset_idx].clone(),
            app.last_active_window,
        )
    };
    let hwnd_val = hwnd.0 as usize;

    // Check audio streaming modes
    let (use_gemini_live_stream, use_parakeet_stream) = {
        let mut gemini = false;
        let mut parakeet = false;

        for block in &preset.blocks {
            if block.block_type == "audio" {
                if let Some(config) = crate::model_config::get_model_by_id(&block.model) {
                    if config.provider == "gemini-live" {
                        gemini = true;
                    }
                    if config.provider == "parakeet" {
                        parakeet = true;
                    }
                }
            }
        }
        (gemini, parakeet)
    };

    std::thread::spawn(move || {
        let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
        let target = last_active_window.map(|h| h.0);

        if use_gemini_live_stream {
            crate::api::record_and_stream_gemini_live(
                preset,
                AUDIO_STOP_SIGNAL.clone(),
                AUDIO_PAUSE_SIGNAL.clone(),
                AUDIO_ABORT_SIGNAL.clone(),
                hwnd,
                target,
            );
        } else if use_parakeet_stream {
            crate::api::audio::record_and_stream_parakeet(
                preset,
                AUDIO_STOP_SIGNAL.clone(),
                AUDIO_PAUSE_SIGNAL.clone(),
                AUDIO_ABORT_SIGNAL.clone(),
                hwnd,
                target,
            );
        } else {
            crate::api::record_audio_and_transcribe(
                preset,
                AUDIO_STOP_SIGNAL.clone(),
                AUDIO_PAUSE_SIGNAL.clone(),
                AUDIO_ABORT_SIGNAL.clone(),
                hwnd,
            );
        }
    });
}
