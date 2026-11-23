// --- RECORDING MODULE ---
// Audio recording overlay with WebView-based waveform visualization.

mod hint;
mod lifecycle;
mod messages;
mod state;
pub(crate) mod ui;
mod window;

use crate::APP;
use state::*;
use std::sync::atomic::Ordering;
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

// Re-export public items
pub use state::CURRENT_RMS;
pub(crate) use state::stop_if_current_recording;
pub(crate) use state::{
    close_if_current_recording, close_recording_session, mark_ready_if_current_recording,
    notify_if_current_recording, recording_session_id, set_initializing_if_current_recording,
    update_viz_if_current_recording,
};

// --- PUBLIC API ---

pub fn is_recording_overlay_active() -> bool {
    RECORDING_STATE.load(Ordering::SeqCst) == 2
}

fn valid_recording_hwnd() -> Option<HWND> {
    let hwnd_val = RECORDING_HWND_VAL.load(Ordering::SeqCst);
    if hwnd_val == 0 {
        return None;
    }
    let hwnd = HWND(hwnd_val as *mut _);
    unsafe {
        if IsWindow(Some(hwnd)).as_bool() {
            Some(hwnd)
        } else {
            RECORDING_HWND_VAL.store(0, Ordering::SeqCst);
            None
        }
    }
}

pub fn stop_recording_and_submit() {
    if is_recording_overlay_active() {
        let was_stopped = is_current_recording_stopped();

        // If already stopped (processing) or aborted, hitting this again should FORCE CLOSE
        if was_stopped {
            let session = cancel_recording_chain();
            if let (Some(hwnd), Some(session)) = (valid_recording_hwnd(), session) {
                unsafe {
                    let _ = PostMessageW(Some(hwnd), WM_APP_HIDE, WPARAM(session), LPARAM(0));
                }
            }
        } else {
            // First time: Just stop and let it process
            stop_current_recording();
            // Force update UI to "Processing"
            if let Some(hwnd) = valid_recording_hwnd() {
                unsafe {
                    let _ = PostMessageW(Some(hwnd), WM_APP_UPDATE_STATE, WPARAM(0), LPARAM(0));
                }
            }
        }
    }
}

pub(crate) fn compositor_ready(session: usize) {
    if let Some(hwnd) = valid_recording_hwnd() {
        unsafe {
            let _ = PostMessageW(Some(hwnd), WM_APP_REAL_SHOW, WPARAM(session), LPARAM(0));
        }
    }
}

pub(crate) fn compositor_toggle_pause(session: usize) {
    if is_recording_overlay_active() {
        toggle_recording_pause_id(session);
    }
}

pub(crate) fn compositor_cancel_session(session: usize) {
    if !is_recording_overlay_active() || !cancel_recording_id(session) {
        return;
    }
    if let Some(hwnd) = valid_recording_hwnd() {
        unsafe {
            let _ = PostMessageW(Some(hwnd), WM_APP_HIDE, WPARAM(session), LPARAM(0));
        }
    }
}

pub(crate) fn compositor_cancel() {
    if !is_recording_overlay_active() {
        return;
    }
    if let Some(session) = cancel_recording_chain() {
        compositor_cancel_session(session);
    }
}

pub fn warmup_recording_overlay() {
    // Transition 0 -> 1
    if RECORDING_STATE
        .compare_exchange(0, 1, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
    {
        RECORDING_HWND_VAL.store(0, Ordering::SeqCst);
        std::thread::spawn(|| {
            window::internal_create_recording_window();
        });
    }
}

pub fn show_recording_overlay(preset_idx: usize) {
    let current = RECORDING_STATE.load(Ordering::SeqCst);
    let hwnd = valid_recording_hwnd();

    // If state is 0 (not started) or 1 (stuck warming up), trigger recovery and auto-show
    if current == 0 || (current == 1 && hwnd.is_none()) {
        // Reset state if stuck
        if current == 1 {
            RECORDING_STATE.store(0, Ordering::SeqCst);
        }

        // Start warmup
        warmup_recording_overlay();

        // Show loading notification
        let ui_lang = APP.lock().unwrap().config.ui_language.clone();
        let locale = crate::gui::locale::LocaleText::get(&ui_lang);
        crate::overlay::auto_copy_badge::show_notification(locale.shell.recording_loading);

        // Spawn a thread to wait for warmup completion and then trigger show
        std::thread::spawn(move || {
            // Poll for up to 5 seconds
            for _ in 0..50 {
                std::thread::sleep(std::time::Duration::from_millis(100));
                if let Some(hwnd) = valid_recording_hwnd() {
                    // Ready! Trigger show
                    unsafe {
                        let _ =
                            PostMessageW(Some(hwnd), WM_APP_SHOW, WPARAM(preset_idx), LPARAM(0));
                    }
                    return;
                }
            }
        });

        return;
    }

    // Wait for HWND to be valid (state is 1 or 2)
    if let Some(hwnd) = hwnd {
        unsafe {
            let _ = PostMessageW(Some(hwnd), WM_APP_SHOW, WPARAM(preset_idx), LPARAM(0));
        }
    } else {
        // HWND not ready yet, reset state and try again
        RECORDING_STATE.store(0, Ordering::SeqCst);
        warmup_recording_overlay();

        let ui_lang = APP.lock().unwrap().config.ui_language.clone();
        let locale = crate::gui::locale::LocaleText::get(&ui_lang);
        crate::overlay::auto_copy_badge::show_notification(locale.shell.recording_loading);
    }
}
