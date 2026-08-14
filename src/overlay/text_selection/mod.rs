// --- TEXT SELECTION MODULE ---
// Badge overlay for text selection with continuous mode support.

mod clipboard;
pub(crate) mod html;
mod state;
mod window;

use crate::APP;
use state::*;
use std::sync::atomic::Ordering;
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

// Re-export public API
pub use clipboard::try_instant_process;
pub use state::TAG_ABORT_SIGNAL;

fn valid_tag_hwnd() -> Option<HWND> {
    let hwnd_val = TAG_HWND.load(Ordering::SeqCst);
    if hwnd_val == 0 {
        return None;
    }

    let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
    unsafe {
        if IsWindow(Some(hwnd)).as_bool() {
            Some(hwnd)
        } else {
            TAG_HWND.store(0, Ordering::SeqCst);
            IS_WARMED_UP.store(false, Ordering::SeqCst);
            IS_WARMING_UP.store(false, Ordering::SeqCst);
            None
        }
    }
}

// --- PUBLIC API ---

pub fn is_active() -> bool {
    TEXT_BADGE_VISIBLE.load(Ordering::SeqCst)
}

pub fn is_processing() -> bool {
    let state = SELECTION_STATE.lock().unwrap();
    state.is_processing
}

/// Check if the trigger hotkey is currently being held down
pub fn is_hotkey_held() -> bool {
    IS_HOTKEY_HELD.load(Ordering::SeqCst)
}

/// Update the badge text to show continuous mode suffix
pub fn update_badge_for_continuous_mode() {
    if let Some(hwnd) = valid_tag_hwnd() {
        unsafe {
            let _ = PostMessageW(Some(hwnd), WM_APP_UPDATE_CONTINUOUS, WPARAM(0), LPARAM(0));
        }
    }
}

/// Hide all badges SYNCHRONOUSLY before screen capture.
pub fn hide_all_badges_for_capture() {
    let _ = crate::overlay::status_compositor::set_selection_capture_visible(false);
}

/// Restore badges after screen capture is complete.
pub fn restore_badges_after_capture() {
    if let Some(hwnd) = valid_tag_hwnd() {
        unsafe {
            let _ = PostMessageW(
                Some(hwnd),
                WM_APP_RESTORE_AFTER_CAPTURE,
                WPARAM(0),
                LPARAM(0),
            );
        }
    }
}

pub fn cancel_selection() {
    let generation = {
        let _transition = SELECTION_TRANSITION_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let generation = SELECTION_LIFECYCLE.cancel();
        reset_selection_internal_state();
        generation
    };
    let hwnd = valid_tag_hwnd();
    if let Some(hwnd) = hwnd {
        unsafe {
            let _ = PostMessageW(
                Some(hwnd),
                WM_APP_HIDE,
                WPARAM(generation as usize),
                LPARAM(0),
            );
        }
    }
}

/// Show or hide the image continuous mode badge
pub fn set_image_continuous_badge(visible: bool) {
    if visible {
        TAG_ABORT_SIGNAL.store(false, Ordering::SeqCst);
    }
    IMAGE_CONTINUOUS_BADGE_VISIBLE.store(visible, Ordering::SeqCst);

    if valid_tag_hwnd().is_none() && !IS_WARMED_UP.load(Ordering::SeqCst) {
        if visible {
            IMAGE_CONTINUOUS_PENDING_SHOW.store(true, Ordering::SeqCst);
        }
        warmup();
        return;
    }

    if let Some(hwnd) = valid_tag_hwnd() {
        unsafe {
            if visible {
                let _ = PostMessageW(Some(hwnd), WM_APP_SHOW_IMAGE_BADGE, WPARAM(0), LPARAM(0));
            } else {
                let _ = PostMessageW(Some(hwnd), WM_APP_HIDE_IMAGE_BADGE, WPARAM(0), LPARAM(0));
            }
        }
    }
}

pub fn warmup() {
    if valid_tag_hwnd().is_some() {
        IS_WARMED_UP.store(true, Ordering::SeqCst);
        return;
    }
    if IS_WARMING_UP
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }

    TAG_HWND.store(0, Ordering::SeqCst);
    IS_WARMED_UP.store(false, Ordering::SeqCst);
    std::thread::spawn(|| {
        window::internal_create_tag_thread();
    });
}

pub fn is_warming_up() -> bool {
    IS_WARMING_UP.load(Ordering::SeqCst)
}

pub fn show_text_selection_tag(preset_idx: usize) {
    let preset_exists = APP
        .lock()
        .map(|app| app.config.presets.get(preset_idx).is_some())
        .unwrap_or(false);
    if !preset_exists {
        crate::log_info!(
            "[TextSelection] Ignoring unavailable preset index {}",
            preset_idx
        );
        cancel_selection();
        return;
    }

    let transition = SELECTION_TRANSITION_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let generation = SELECTION_LIFECYCLE.begin();

    // Record when and for which preset the badge is being shown
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    LAST_BADGE_SHOW_TIME.store(now, Ordering::SeqCst);
    LAST_BADGE_PRESET_IDX.store(preset_idx, Ordering::SeqCst);

    // Prepare State
    {
        let mut state = SELECTION_STATE.lock().unwrap();
        if !SELECTION_LIFECYCLE.is_current(generation) {
            return;
        }
        state.preset_idx = Some(preset_idx);
        state.generation = generation;
        state.is_selecting = false;
        state.is_processing = false;
        TEXT_BADGE_VISIBLE.store(true, Ordering::SeqCst);
        TAG_ABORT_SIGNAL.store(false, Ordering::SeqCst);

        if !crate::overlay::continuous_mode::is_active() {
            CONTINUOUS_ACTIVATED_THIS_SESSION.store(false, Ordering::SeqCst);
            HOLD_DETECTED_THIS_SESSION.store(false, Ordering::SeqCst);
        }
        if let Some((mods, vk)) = crate::overlay::continuous_mode::get_current_hotkey_info() {
            unsafe {
                TRIGGER_MODIFIERS = mods;
                TRIGGER_VK_CODE = vk;

                use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
                if !crate::overlay::continuous_mode::is_active() {
                    let is_physically_held = (GetAsyncKeyState(vk as i32) as u16 & 0x8000) != 0;
                    IS_HOTKEY_HELD.store(is_physically_held, Ordering::SeqCst);
                }
            }
        } else {
            IS_HOTKEY_HELD.store(false, Ordering::SeqCst);
        }
    }
    drop(transition);

    // Signal show immediately, or preserve this exact selection generation through warmup.
    if let Some(hwnd) = valid_tag_hwnd() {
        post_text_selection_show(hwnd, generation);
    } else if !IS_WARMED_UP.load(Ordering::SeqCst) && SELECTION_LIFECYCLE.queue_show(generation) {
        warmup();
        if let Some(hwnd) = valid_tag_hwnd() {
            dispatch_pending_text_selection_show(hwnd);
        }
    }
}

pub(super) fn post_text_selection_show(hwnd: HWND, generation: u64) {
    unsafe {
        let _ = PostMessageW(
            Some(hwnd),
            WM_APP_SHOW,
            WPARAM(generation as usize),
            LPARAM(0),
        );
    }
}

pub(super) fn dispatch_pending_text_selection_show(hwnd: HWND) {
    if let Some(generation) = SELECTION_LIFECYCLE.take_pending_show() {
        post_text_selection_show(hwnd, generation);
    }
}
