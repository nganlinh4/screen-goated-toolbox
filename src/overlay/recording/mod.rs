// --- RECORDING MODULE ---
// Audio recording overlay with WebView-based waveform visualization.

mod messages;
mod state;
mod ui;
mod window;

use crate::APP;
use state::*;
use std::sync::atomic::Ordering;
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

// Re-export public items
pub use state::{
    AUDIO_ABORT_SIGNAL, AUDIO_INITIALIZING, AUDIO_PAUSE_SIGNAL, AUDIO_STOP_SIGNAL,
    AUDIO_WARMUP_COMPLETE, CURRENT_RMS, update_audio_viz,
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
        let was_stopped = AUDIO_STOP_SIGNAL.load(Ordering::SeqCst);

        // If already stopped (processing) or aborted, hitting this again should FORCE CLOSE
        if was_stopped {
            AUDIO_ABORT_SIGNAL.store(true, Ordering::SeqCst);
            if let Some(hwnd) = valid_recording_hwnd() {
                unsafe {
                    let _ = PostMessageW(Some(hwnd), WM_APP_HIDE, WPARAM(0), LPARAM(0));
                }
            }
        } else {
            // First time: Just stop and let it process
            AUDIO_STOP_SIGNAL.store(true, Ordering::SeqCst);
            // Force update UI to "Processing"
            if let Some(hwnd) = valid_recording_hwnd() {
                unsafe {
                    let _ = PostMessageW(Some(hwnd), WM_APP_UPDATE_STATE, WPARAM(0), LPARAM(0));
                }
            }
        }
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
        // Reset Signals
        AUDIO_STOP_SIGNAL.store(false, Ordering::SeqCst);
        AUDIO_PAUSE_SIGNAL.store(false, Ordering::SeqCst);
        AUDIO_ABORT_SIGNAL.store(false, Ordering::SeqCst);
        AUDIO_WARMUP_COMPLETE.store(false, Ordering::SeqCst);
        CURRENT_RMS.store(0, Ordering::Relaxed);

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
