//! Overlay lifecycle management (show/stop/check active)

use super::state::*;
use super::webview::*;
use super::wndproc::*;
use crate::APP;
use crate::api::realtime_audio::start_realtime_transcription;
use std::sync::atomic::{AtomicIsize, Ordering};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::{
    DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Gdi::HBRUSH;
use windows::Win32::System::Com::{CoInitialize, CoUninitialize};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::w;

static PENDING_REALTIME_START_PRESET: AtomicIsize = AtomicIsize::new(-1);

pub fn is_realtime_overlay_active() -> bool {
    if crate::overlay::realtime_egui::MINIMAL_ACTIVE.load(Ordering::SeqCst)
        || crate::overlay::realtime_egui::MINIMAL_STOPPING.load(Ordering::SeqCst)
    {
        return true;
    }

    unsafe {
        if !IS_ACTIVE {
            return false;
        }
        let hwnd = std::ptr::addr_of!(REALTIME_HWND).read();
        if hwnd.is_invalid() || !IsWindow(Some(hwnd)).as_bool() {
            IS_ACTIVE = false;
            REALTIME_SESSION_STOPPING.store(false, Ordering::SeqCst);
            REALTIME_STOP_SIGNAL.store(false, Ordering::SeqCst);
            return false;
        }
        true
    }
}

/// Stop the realtime overlay and hide windows
pub fn stop_realtime_overlay() {
    if crate::overlay::realtime_egui::MINIMAL_ACTIVE.load(Ordering::SeqCst)
        || crate::overlay::realtime_egui::MINIMAL_STOPPING.load(Ordering::SeqCst)
    {
        crate::overlay::realtime_egui::stop_minimal_overlay();
        return;
    }

    super::controller::stop_runtime_flags();

    unsafe {
        let hwnd = std::ptr::addr_of!(REALTIME_HWND).read();
        if !hwnd.is_invalid() && IsWindow(Some(hwnd)).as_bool() {
            let _ = PostMessageW(Some(hwnd), WM_APP_REALTIME_HIDE, WPARAM(0), LPARAM(0));
        } else {
            IS_ACTIVE = false;
            REALTIME_SESSION_STOPPING.store(false, Ordering::SeqCst);
            REALTIME_STOP_SIGNAL.store(false, Ordering::SeqCst);
        }
    }
}

pub fn show_realtime_overlay(preset_idx: usize) {
    if crate::overlay::realtime_egui::recently_stopped_minimal(preset_idx) {
        return;
    }

    let realtime_window_mode = APP
        .lock()
        .map(|app| {
            app.config
                .presets
                .get(preset_idx)
                .map(|preset| preset.realtime_window_mode.clone())
                .unwrap_or_default()
        })
        .unwrap_or_default();

    if realtime_window_mode == "minimal" {
        crate::overlay::realtime_egui::show_realtime_egui_overlay(preset_idx);
        return;
    }

    if crate::overlay::realtime_egui::MINIMAL_STOPPING.load(Ordering::SeqCst) {
        return;
    }

    let capability = crate::runtime_support::require_webview2("Realtime overlay");
    if !capability.is_supported() {
        crate::runtime_support::notify_capability_issue(&capability);
        return;
    }

    unsafe {
        if REALTIME_SESSION_STOPPING.load(Ordering::SeqCst) {
            let hwnd = std::ptr::addr_of!(REALTIME_HWND).read();
            if crate::overlay::realtime_egui::MINIMAL_STOPPING.load(Ordering::SeqCst) {
                return;
            } else if IS_ACTIVE && !hwnd.is_invalid() && IsWindow(Some(hwnd)).as_bool() {
                return;
            } else {
                REALTIME_SESSION_STOPPING.store(false, Ordering::SeqCst);
                REALTIME_STOP_SIGNAL.store(false, Ordering::SeqCst);
                IS_ACTIVE = false;
            }
        }

        // Initialize on-demand if not warmed up
        if !IS_WARMED_UP {
            PENDING_REALTIME_START_PRESET.store(preset_idx as isize, Ordering::SeqCst);
            if !IS_INITIALIZING {
                IS_INITIALIZING = true;
                std::thread::spawn(move || {
                    internal_create_realtime_loop();
                });
            }
            return;
        }

        if !std::ptr::addr_of!(REALTIME_HWND).read().is_invalid() {
            let _ = PostMessageW(
                Some(REALTIME_HWND),
                WM_APP_REALTIME_START,
                WPARAM(preset_idx),
                LPARAM(0),
            );
        }
    }
}

unsafe fn internal_create_realtime_loop() {
    unsafe {
        let _ = CoInitialize(None); // Required for WebView
        let instance = GetModuleHandleW(None).unwrap();

        // --- Register Classes ---
        let class_name = w!("RealtimeWebViewOverlay");
        REGISTER_REALTIME_CLASS.call_once(|| {
            let wc = WNDCLASSW {
                lpfnWndProc: Some(realtime_wnd_proc_internal),
                hInstance: instance.into(),
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap(),
                lpszClassName: class_name,
                style: CS_HREDRAW | CS_VREDRAW,
                hbrBackground: HBRUSH(std::ptr::null_mut()),
                ..Default::default()
            };
            let _ = RegisterClassW(&wc);
        });

        let trans_class = w!("RealtimeTranslationWebViewOverlay");
        REGISTER_TRANSLATION_CLASS.call_once(|| {
            let wc = WNDCLASSW {
                lpfnWndProc: Some(translation_wnd_proc_internal),
                hInstance: instance.into(),
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap(),
                lpszClassName: trans_class,
                style: CS_HREDRAW | CS_VREDRAW,
                hbrBackground: HBRUSH(std::ptr::null_mut()),
                ..Default::default()
            };
            let _ = RegisterClassW(&wc);
        });

        // Create windows hidden
        let main_hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            class_name,
            w!("Realtime Transcription"),
            WS_POPUP, // Hidden initially
            0,
            0,
            100,
            100,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .unwrap();

        let trans_hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            trans_class,
            w!("Translation"),
            WS_POPUP, // Hidden initially
            0,
            0,
            100,
            100,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .unwrap();

        // Enable rounded corners (Windows 11+)
        let corner_pref = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(
            main_hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &corner_pref as *const _ as *const std::ffi::c_void,
            std::mem::size_of_val(&corner_pref) as u32,
        );
        let _ = DwmSetWindowAttribute(
            trans_hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &corner_pref as *const _ as *const std::ffi::c_void,
            std::mem::size_of_val(&corner_pref) as u32,
        );

        REALTIME_HWND = main_hwnd;
        TRANSLATION_HWND = trans_hwnd;

        // Create WebViews
        create_realtime_webview(
            main_hwnd,
            false,
            "device",
            "English",
            "google-gtx",
            "gemini",
            16,
        );
        create_realtime_webview(
            trans_hwnd,
            true,
            "device",
            "English",
            "google-gtx",
            "gemini",
            16,
        );

        // Mark as warmed up and ready
        IS_WARMED_UP = true;
        let pending_preset = PENDING_REALTIME_START_PRESET.swap(-1, Ordering::SeqCst);
        if pending_preset >= 0 {
            let _ = PostMessageW(
                Some(REALTIME_HWND),
                WM_APP_REALTIME_START,
                WPARAM(pending_preset as usize),
                LPARAM(0),
            );
        }

        // Message loop
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
            if msg.message == WM_QUIT {
                break;
            }
        }

        // Cleanup
        destroy_realtime_webview(REALTIME_HWND);
        destroy_realtime_webview(TRANSLATION_HWND);
        IS_ACTIVE = false;
        IS_WARMED_UP = false;
        IS_INITIALIZING = false;
        PENDING_REALTIME_START_PRESET.store(-1, Ordering::SeqCst);
        REALTIME_SESSION_STOPPING.store(false, Ordering::SeqCst);
        REALTIME_STOP_SIGNAL.store(false, Ordering::SeqCst);
        REALTIME_HWND = HWND::default();
        TRANSLATION_HWND = HWND::default();
        CoUninitialize();
    }
}

unsafe extern "system" fn realtime_wnd_proc_internal(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        if msg == WM_APP_REALTIME_START {
            let preset_idx = wparam.0;
            handle_start_overlay(preset_idx);
            return LRESULT(0);
        }
        realtime_wnd_proc(hwnd, msg, wparam, lparam)
    }
}

unsafe extern "system" fn translation_wnd_proc_internal(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe { translation_wnd_proc(hwnd, msg, wparam, lparam) }
}

unsafe fn handle_start_overlay(preset_idx: usize) {
    unsafe {
        if IS_ACTIVE || REALTIME_SESSION_STOPPING.load(Ordering::SeqCst) {
            return;
        }

        let mut preset = APP.lock().unwrap().config.presets[preset_idx].clone();

        // Check if Minimal Mode
        if preset.realtime_window_mode == "minimal" {
            crate::overlay::realtime_egui::show_realtime_egui_overlay(preset_idx);
            return;
        }

        let session_config = super::controller::load_session_config();
        let (trans_size, transcription_size) = {
            let app = APP.lock().unwrap();
            (
                app.config.realtime_translation_size,
                app.config.realtime_transcription_size,
            )
        };
        super::controller::reset_runtime_for_new_session();

        let target_language = if !session_config.target_language.is_empty() {
            session_config.target_language.clone()
        } else if preset.blocks.len() > 1 {
            let trans_block = &preset.blocks[1];
            if !trans_block.selected_language.is_empty() {
                trans_block.selected_language.clone()
            } else {
                trans_block
                    .language_vars
                    .get("language")
                    .cloned()
                    .or_else(|| trans_block.language_vars.get("language1").cloned())
                    .unwrap_or_else(|| "English".to_string())
            }
        } else {
            "English".to_string()
        };

        let mut active_config = session_config.clone();
        active_config.target_language = target_language.clone();
        super::controller::apply_session_config(&active_config);
        preset.audio_source = active_config.audio_source.clone();

        // Calculate positions
        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let screen_h = GetSystemMetrics(SM_CYSCREEN);
        let has_translation = preset.blocks.len() > 1;
        let main_w = transcription_size.0;
        let main_h = transcription_size.1;
        let trans_w = trans_size.0;
        let trans_h = trans_size.1;

        let (main_x, main_y) = if has_translation {
            let total_w = main_w + trans_w + GAP;
            ((screen_w - total_w) / 2, (screen_h - main_h) / 2)
        } else {
            ((screen_w - main_w) / 2, (screen_h - main_h) / 2)
        };

        // Update window positions and sizes
        let _ = SetWindowPos(
            REALTIME_HWND,
            Some(HWND_TOPMOST),
            main_x,
            main_y,
            main_w,
            main_h,
            SWP_SHOWWINDOW,
        );
        if has_translation {
            let trans_x = main_x + main_w + GAP;
            let _ = SetWindowPos(
                TRANSLATION_HWND,
                Some(HWND_TOPMOST),
                trans_x,
                main_y,
                trans_w,
                trans_h,
                SWP_SHOWWINDOW,
            );
        } else {
            let _ = ShowWindow(TRANSLATION_HWND, SW_HIDE);
        }

        // Notify WebViews of new settings
        notify_webview_settings(
            REALTIME_HWND,
            &active_config.audio_source,
            &target_language,
            &active_config.translation_model,
            &active_config.transcription_model,
            &active_config.transcription_language,
            active_config.font_size,
        );

        // Explicitly resize WebViews to match window sizes
        resize_webview(REALTIME_HWND, main_w, main_h);

        // Clear text to start fresh
        clear_webview_text(REALTIME_HWND);

        if has_translation {
            notify_webview_settings(
                TRANSLATION_HWND,
                "mic",
                &target_language,
                &active_config.translation_model,
                &active_config.transcription_model,
                &active_config.transcription_language,
                active_config.font_size,
            );
            resize_webview(TRANSLATION_HWND, trans_w, trans_h);
            clear_webview_text(TRANSLATION_HWND);
        }

        // Sync visibility state to webviews (fixes toggled->hidden state on re-show)
        sync_visibility_to_webviews();

        // Start transcription
        let trans_hwnd_opt = if has_translation {
            Some(TRANSLATION_HWND)
        } else {
            None
        };
        start_realtime_transcription(
            preset,
            REALTIME_STOP_SIGNAL.clone(),
            REALTIME_HWND,
            trans_hwnd_opt,
            REALTIME_STATE.clone(),
        );
    }
}

fn notify_webview_settings(
    hwnd: HWND,
    source: &str,
    lang: &str,
    model: &str,
    trans_model: &str,
    trans_lang: &str,
    font_size: u32,
) {
    let hwnd_key = hwnd.0 as isize;
    let script = format!(
        "if(window.updateSettings) window.updateSettings({{ audioSource: '{}', targetLanguage: '{}', translationModel: '{}', transcriptionModel: '{}', transcriptionLanguage: '{}', fontSize: {} }});",
        source,
        lang,
        model,
        trans_model,
        trans_lang.to_uppercase(),
        font_size
    );
    REALTIME_WEBVIEWS.with(|wvs| {
        if let Some(webview) = wvs.borrow().get(&hwnd_key) {
            let _ = webview.evaluate_script(&script);
        }
    });
}

fn resize_webview(hwnd: HWND, width: i32, height: i32) {
    let hwnd_key = hwnd.0 as isize;
    REALTIME_WEBVIEWS.with(|wvs| {
        if let Some(webview) = wvs.borrow().get(&hwnd_key) {
            let _ = webview.set_bounds(wry::Rect {
                position: wry::dpi::Position::Physical(wry::dpi::PhysicalPosition::new(0, 0)),
                size: wry::dpi::Size::Physical(wry::dpi::PhysicalSize::new(
                    width as u32,
                    height as u32,
                )),
            });
        }
    });
}
