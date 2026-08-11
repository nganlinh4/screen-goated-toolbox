//! Overlay lifecycle management (show/stop/check active)

use super::state::*;
use super::webview::*;
use super::wndproc::*;
use super::{layout, layout::CardRole};
use crate::APP;
use crate::api::realtime_audio::start_realtime_transcription;
use std::sync::atomic::{AtomicIsize, Ordering};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::DwmExtendFrameIntoClientArea;
use windows::Win32::Graphics::Gdi::{CreateRectRgn, HBRUSH, SetWindowRgn};
use windows::Win32::System::Com::{CoInitialize, CoUninitialize};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::w;

static PENDING_REALTIME_START_PRESET: AtomicIsize = AtomicIsize::new(-1);

fn session_transition_in_progress(pending_preset: isize, stopping: bool) -> bool {
    pending_preset >= 0 || stopping
}

pub fn is_realtime_overlay_active() -> bool {
    if crate::overlay::realtime_egui::MINIMAL_ACTIVE.load(Ordering::SeqCst)
        || crate::overlay::realtime_egui::MINIMAL_STOPPING.load(Ordering::SeqCst)
    {
        return true;
    }

    if session_transition_in_progress(
        PENDING_REALTIME_START_PRESET.load(Ordering::SeqCst),
        REALTIME_SESSION_STOPPING.load(Ordering::SeqCst),
    ) {
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

/// Stop the realtime overlay and close its compositor.
pub fn stop_realtime_overlay() {
    if crate::overlay::realtime_egui::MINIMAL_ACTIVE.load(Ordering::SeqCst)
        || crate::overlay::realtime_egui::MINIMAL_STOPPING.load(Ordering::SeqCst)
    {
        crate::overlay::realtime_egui::stop_minimal_overlay();
        return;
    }

    PENDING_REALTIME_START_PRESET.store(-1, Ordering::SeqCst);
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
            if crate::overlay::realtime_egui::MINIMAL_STOPPING.load(Ordering::SeqCst)
                || (IS_ACTIVE && !hwnd.is_invalid() && IsWindow(Some(hwnd)).as_bool())
            {
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

        let virtual_x = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let virtual_y = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let virtual_width = GetSystemMetrics(SM_CXVIRTUALSCREEN).max(1);
        let virtual_height = GetSystemMetrics(SM_CYVIRTUALSCREEN).max(1);

        // The one host spans the virtual desktop; its native region is reduced
        // to the visible card rectangles so the space between cards stays inert.
        let main_hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            class_name,
            w!("Realtime compositor"),
            WS_POPUP | WS_CLIPCHILDREN,
            virtual_x,
            virtual_y,
            virtual_width,
            virtual_height,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .unwrap();
        let margins = MARGINS {
            cxLeftWidth: -1,
            cxRightWidth: -1,
            cyTopHeight: -1,
            cyBottomHeight: -1,
        };
        let _ = DwmExtendFrameIntoClientArea(main_hwnd, &margins);
        let empty = CreateRectRgn(0, 0, 0, 0);
        let _ = SetWindowRgn(main_hwnd, Some(empty), false);

        REALTIME_HWND = main_hwnd;

        if let Err(error) =
            create_realtime_webview(main_hwnd, "device", "English", "google-gtx", "gemini", 16)
        {
            crate::log_info!("[RealtimeCompositor] WebView creation failed: {error:#}");
            let _ = DestroyWindow(main_hwnd);
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
        IS_ACTIVE = false;
        IS_WARMED_UP = false;
        IS_INITIALIZING = false;
        PENDING_REALTIME_START_PRESET.store(-1, Ordering::SeqCst);
        REALTIME_SESSION_STOPPING.store(false, Ordering::SeqCst);
        REALTIME_STOP_SIGNAL.store(false, Ordering::SeqCst);
        REALTIME_HWND = HWND::default();
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

pub(super) fn on_compositor_ready(hwnd: HWND) {
    unsafe {
        if hwnd != std::ptr::addr_of!(REALTIME_HWND).read() {
            return;
        }
        IS_WARMED_UP = true;
        crate::log_info!("[RealtimeCompositor] both cards ready");
        let pending_preset = PENDING_REALTIME_START_PRESET.swap(-1, Ordering::SeqCst);
        if pending_preset >= 0 {
            let _ = PostMessageW(
                Some(hwnd),
                WM_APP_REALTIME_START,
                WPARAM(pending_preset as usize),
                LPARAM(0),
            );
        }
    }
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

        layout::configure(
            (main_x, main_y),
            (main_w, main_h),
            (trans_w, trans_h),
            has_translation,
        );
        sync_compositor_layout(REALTIME_HWND);

        notify_card_settings(
            CardRole::Transcription,
            &active_config.audio_source,
            &target_language,
            &active_config.translation_model,
            &active_config.transcription_model,
            &active_config.transcription_language,
            active_config.font_size,
        );

        // Clear text to start fresh
        clear_card_text(CardRole::Transcription);

        if has_translation {
            notify_card_settings(
                CardRole::Translation,
                "mic",
                &target_language,
                &active_config.translation_model,
                &active_config.transcription_model,
                &active_config.transcription_language,
                active_config.font_size,
            );
            clear_card_text(CardRole::Translation);
        }

        // Sync visibility state to webviews (fixes toggled->hidden state on re-show)
        sync_visibility_to_webview();

        // Start transcription
        let trans_hwnd_opt = has_translation.then_some(REALTIME_HWND);
        start_realtime_transcription(
            preset,
            REALTIME_STOP_SIGNAL.clone(),
            REALTIME_HWND,
            trans_hwnd_opt,
            REALTIME_STATE.clone(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::session_transition_in_progress;

    #[test]
    fn pending_or_stopping_session_remains_available_to_the_toggle() {
        assert!(session_transition_in_progress(0, false));
        assert!(session_transition_in_progress(42, false));
        assert!(session_transition_in_progress(-1, true));
        assert!(!session_transition_in_progress(-1, false));
    }
}
