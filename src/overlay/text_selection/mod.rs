// --- TEXT SELECTION MODULE ---
// Badge overlay for text selection with continuous mode support.

mod clipboard;
mod html;
mod state;
mod window;

use state::*;
use std::sync::atomic::Ordering;
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

// Re-export public API
pub use clipboard::try_instant_process;
pub use state::TAG_ABORT_SIGNAL;

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
    let hwnd_val = TAG_HWND.load(Ordering::SeqCst);
    if hwnd_val != 0 {
        unsafe {
            let hwnd = HWND(hwnd_val as *mut _);
            let _ = PostMessageW(Some(hwnd), WM_APP_UPDATE_CONTINUOUS, WPARAM(0), LPARAM(0));
        }
    }
}

/// Hide all badges SYNCHRONOUSLY before screen capture.
pub fn hide_all_badges_for_capture() {
    let hwnd_val = TAG_HWND.load(Ordering::SeqCst);
    if hwnd_val != 0 {
        unsafe {
            let hwnd = HWND(hwnd_val as *mut _);
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
    }
}

/// Restore badges after screen capture is complete.
pub fn restore_badges_after_capture() {
    let hwnd_val = TAG_HWND.load(Ordering::SeqCst);
    if hwnd_val != 0 {
        unsafe {
            let hwnd = HWND(hwnd_val as *mut _);
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
    crate::log_info!("[Badge] cancel_selection() called");
    reset_selection_internal_state();
    let hwnd_val = TAG_HWND.load(Ordering::SeqCst);
    crate::log_info!("[Badge] cancel_selection: hwnd_val={}", hwnd_val);
    if hwnd_val != 0 {
        unsafe {
            let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
            crate::log_info!("[Badge] cancel_selection: posting WM_APP_HIDE");
            let _ = PostMessageW(Some(hwnd), WM_APP_HIDE, WPARAM(0), LPARAM(0));
        }
    }
}

/// Show or hide the image continuous mode badge
pub fn set_image_continuous_badge(visible: bool) {
    crate::log_info!("[Badge] set_image_continuous_badge(visible={})", visible);
    if visible {
        TAG_ABORT_SIGNAL.store(false, Ordering::SeqCst);
    }
    IMAGE_CONTINUOUS_BADGE_VISIBLE.store(visible, Ordering::SeqCst);

    if !IS_WARMED_UP.load(Ordering::SeqCst) {
        if visible {
            IMAGE_CONTINUOUS_PENDING_SHOW.store(true, Ordering::SeqCst);
        }
        warmup();
        return;
    }

    let hwnd_val = TAG_HWND.load(Ordering::SeqCst);
    if hwnd_val != 0 {
        unsafe {
            let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
            if visible {
                let mut pt = POINT::default();
                let _ = GetCursorPos(&mut pt);
                let target_x = pt.x + OFFSET_X;
                let target_y = pt.y + OFFSET_Y;
                let _ = MoveWindow(hwnd, target_x, target_y, BADGE_WIDTH, BADGE_HEIGHT, false);

                let _ = PostMessageW(Some(hwnd), WM_APP_SHOW_IMAGE_BADGE, WPARAM(0), LPARAM(0));
            } else {
                let _ = PostMessageW(Some(hwnd), WM_APP_HIDE_IMAGE_BADGE, WPARAM(0), LPARAM(0));
            }
        }
    }
}

pub fn warmup() {
    if IS_WARMED_UP.load(Ordering::SeqCst) {
        return;
    }
    if IS_WARMING_UP
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }

    std::thread::spawn(|| {
        window::internal_create_tag_thread();
    });
}

pub fn is_warming_up() -> bool {
    IS_WARMING_UP.load(Ordering::SeqCst)
}

pub fn show_text_selection_tag(preset_idx: usize) {
    TEXT_BADGE_VISIBLE.store(true, Ordering::SeqCst);
    TAG_ABORT_SIGNAL.store(false, Ordering::SeqCst);

    // Record when and for which preset the badge is being shown
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    LAST_BADGE_SHOW_TIME.store(now, Ordering::SeqCst);
    LAST_BADGE_PRESET_IDX.store(preset_idx, Ordering::SeqCst);

    // Ensure Warmed Up / Trigger Warmup
    if !IS_WARMED_UP.load(Ordering::SeqCst) {
        PENDING_SHOW_ON_WARMUP.store(true, Ordering::SeqCst);
        warmup();
    }

    // Prepare State
    {
        let mut state = SELECTION_STATE.lock().unwrap();
        state.preset_idx = preset_idx;
        state.is_selecting = false;
        state.is_processing = false;
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

    // Signal Show
    let hwnd_val = TAG_HWND.load(Ordering::SeqCst);
    if hwnd_val != 0 {
        unsafe {
            let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);

            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);
            let target_x = pt.x + OFFSET_X;
            let target_y = pt.y + OFFSET_Y;

            let _ = MoveWindow(hwnd, target_x, target_y, BADGE_WIDTH, BADGE_HEIGHT, false);

            let _ = PostMessageW(Some(hwnd), WM_APP_SHOW, WPARAM(0), LPARAM(0));
        }
    }
}
