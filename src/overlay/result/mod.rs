pub mod button_canvas;
mod event_handler;
pub mod layout;
mod logic;
pub mod markdown_view;
pub mod paint;
mod refine;
mod restore;
pub mod state;
mod window;

pub use refine::{trigger_edit, trigger_refine_cancel, trigger_refine_submit};
pub use state::{
    ChainCancelToken, RefineContext, WINDOW_STATES, WindowType, close_chain_windows, link_windows,
};
pub use window::{ResultWindowParams, create_result_window, get_chain_color, update_window_text};

// Trigger functions for button canvas IPC
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{IsWindow, PostMessageW, WM_CLOSE};

// Helper to check if any window is currently refining/editing
pub fn is_any_refine_active() -> bool {
    let states = WINDOW_STATES.lock().unwrap();
    states.values().any(|s| s.is_editing)
}

// Helper to get the parent HWND of the active refine session
pub fn get_active_refine_parent() -> Option<HWND> {
    let states = WINDOW_STATES.lock().unwrap();
    states
        .iter()
        .find(|(_, s)| s.is_editing)
        .map(|(hwnd, _)| HWND(*hwnd as *mut std::ffi::c_void))
}

// Helper to update refine text
pub fn set_refine_text(hwnd: HWND, text: &str, is_insert: bool) {
    button_canvas::send_refine_text_update(hwnd, text, is_insert);

    // Only update internal state if overwriting (for consistency)
    if !is_insert {
        let hwnd_key = hwnd.0 as isize;
        let mut states = WINDOW_STATES.lock().unwrap();
        if let Some(state) = states.get_mut(&hwnd_key) {
            state.input_text = text.to_string();
        }
    }
}

/// Trigger copy action on a result window
pub fn trigger_copy(hwnd: HWND) {
    let hwnd_key = hwnd.0 as isize;

    // Get text and copy to clipboard
    let text = {
        let states = WINDOW_STATES.lock().unwrap();
        states
            .get(&hwnd_key)
            .map(|s| s.full_text.clone())
            .unwrap_or_default()
    };

    if !text.is_empty() {
        crate::overlay::utils::copy_to_clipboard(&text, hwnd);

        // Set copy success flag
        {
            let mut states = WINDOW_STATES.lock().unwrap();
            if let Some(state) = states.get_mut(&hwnd_key) {
                state.copy_success = true;
            }
        }

        // Update canvas to show success state
        button_canvas::update_window_position(hwnd);

        // Reset success flag after delay
        let hwnd_val = hwnd.0 as usize;
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(1500));
            {
                let mut states = WINDOW_STATES.lock().unwrap();
                if let Some(state) = states.get_mut(&(hwnd_val as isize)) {
                    state.copy_success = false;
                }
            }
            // Update canvas after dropping lock
            let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
            unsafe {
                if IsWindow(Some(hwnd)).as_bool() {
                    button_canvas::update_window_position(hwnd);
                }
            }
        });
    }
}

/// Trigger undo action on a result window
pub fn trigger_undo(hwnd: HWND) {
    let hwnd_key = hwnd.0 as isize;

    let (prev_text, is_markdown) = {
        let mut states = WINDOW_STATES.lock().unwrap();
        if let Some(state) = states.get_mut(&hwnd_key) {
            if let Some(last) = state.text_history.pop() {
                let current = state.full_text.clone();
                state.redo_history.push(current);
                state.full_text = last.clone();
                (Some(last), state.is_markdown_mode)
            } else {
                (None, false)
            }
        } else {
            (None, false)
        }
    };

    if let Some(txt) = prev_text {
        // Update window text
        let wide_text = crate::overlay::utils::to_wstring(&txt);
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::SetWindowTextW(
                hwnd,
                windows::core::PCWSTR(wide_text.as_ptr()),
            );
        }

        if is_markdown {
            unsafe {
                let _ = PostMessageW(
                    Some(hwnd),
                    event_handler::misc::WM_CREATE_WEBVIEW,
                    WPARAM(0),
                    LPARAM(0),
                );
            }
        }

        // Update canvas
        button_canvas::update_window_position(hwnd);
    }
}

/// Trigger redo action on a result window
pub fn trigger_redo(hwnd: HWND) {
    let hwnd_key = hwnd.0 as isize;

    let (next_text, is_markdown) = {
        let mut states = WINDOW_STATES.lock().unwrap();
        if let Some(state) = states.get_mut(&hwnd_key) {
            if let Some(redo) = state.redo_history.pop() {
                let current = state.full_text.clone();
                state.text_history.push(current);
                state.full_text = redo.clone();
                (Some(redo), state.is_markdown_mode)
            } else {
                (None, false)
            }
        } else {
            (None, false)
        }
    };

    if let Some(txt) = next_text {
        let wide_text = crate::overlay::utils::to_wstring(&txt);
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::SetWindowTextW(
                hwnd,
                windows::core::PCWSTR(wide_text.as_ptr()),
            );
        }

        if is_markdown {
            unsafe {
                let _ = PostMessageW(
                    Some(hwnd),
                    event_handler::misc::WM_CREATE_WEBVIEW,
                    WPARAM(0),
                    LPARAM(0),
                );
            }
        }

        button_canvas::update_window_position(hwnd);
    }
}

/// Trigger markdown toggle (switch back to plain text)
pub fn trigger_markdown_toggle(hwnd: HWND) {
    let hwnd_key = hwnd.0 as isize;

    // Check if we can toggle
    let can_toggle = {
        let states = WINDOW_STATES.lock().unwrap();
        states
            .get(&hwnd_key)
            .map(|s| !s.is_refining && !s.is_streaming_active)
            .unwrap_or(false)
    };

    if !can_toggle {
        return;
    }

    // Toggle the mode in state
    let is_now_markdown = {
        let mut states = WINDOW_STATES.lock().unwrap();
        if let Some(state) = states.get_mut(&hwnd_key) {
            state.is_markdown_mode = !state.is_markdown_mode;
            state.is_markdown_mode
        } else {
            return;
        }
    };

    // Use message passing to update UI on the correct thread
    unsafe {
        if is_now_markdown {
            let _ = PostMessageW(
                Some(hwnd),
                event_handler::misc::WM_CREATE_WEBVIEW,
                WPARAM(0),
                LPARAM(0),
            );
        } else {
            // Switching BACK to plain text
            // We must manually update the window text because the optimized streaming path skipped it!
            let full_text = {
                let states = WINDOW_STATES.lock().unwrap();
                states
                    .get(&hwnd_key)
                    .map(|s| s.full_text.clone())
                    .unwrap_or_default()
            };
            let wide_text = crate::overlay::utils::to_wstring(&full_text);
            let _ = windows::Win32::UI::WindowsAndMessaging::SetWindowTextW(
                hwnd,
                windows::core::PCWSTR(wide_text.as_ptr()),
            );

            let _ = PostMessageW(
                Some(hwnd),
                event_handler::misc::WM_HIDE_MARKDOWN,
                WPARAM(0),
                LPARAM(0),
            );
        }
    }

    // Update canvas to reflect the new state (e.g., active icon state)
    button_canvas::update_window_position(hwnd);
}

/// Trigger speaker/TTS
pub fn trigger_speaker(hwnd: HWND) {
    let hwnd_key = hwnd.0 as isize;
    crate::log_info!("[TTS] trigger_speaker called for hwnd: {}", hwnd_key);

    let (full_text, current_tts_id, is_loading, state_exists) = {
        let states = WINDOW_STATES.lock().unwrap();
        if let Some(s) = states.get(&hwnd_key) {
            (s.full_text.clone(), s.tts_request_id, s.tts_loading, true)
        } else {
            (String::new(), 0, false, false)
        }
    };

    if !state_exists {
        crate::log_info!(
            "[TTS] ERROR: Window state not found for hwnd: {} - window may have been closed",
            hwnd_key
        );
        return;
    }

    if is_loading {
        crate::log_info!(
            "[TTS] Ignoring click - already loading (tts_request_id: {})",
            current_tts_id
        );
        return;
    }

    if current_tts_id != 0 && crate::api::tts::TTS_MANAGER.is_speaking(current_tts_id) {
        // Stop speaking
        crate::log_info!(
            "[TTS] Stopping current speech (request_id: {})",
            current_tts_id
        );
        crate::api::tts::TTS_MANAGER.stop();
        {
            let mut states = WINDOW_STATES.lock().unwrap();
            if let Some(state) = states.get_mut(&hwnd_key) {
                state.tts_request_id = 0;
                state.tts_loading = false;
            }
        }
    } else if !full_text.is_empty() {
        // Start speaking
        crate::log_info!(
            "[TTS] Starting speech - text length: {} chars",
            full_text.len()
        );
        {
            let mut states = WINDOW_STATES.lock().unwrap();
            if let Some(state) = states.get_mut(&hwnd_key) {
                state.tts_loading = true;
            }
        }

        let request_id = crate::api::tts::TTS_MANAGER.speak(&full_text, hwnd_key);
        crate::log_info!(
            "[TTS] TTS_MANAGER.speak returned request_id: {}",
            request_id
        );
        {
            let mut states = WINDOW_STATES.lock().unwrap();
            if let Some(state) = states.get_mut(&hwnd_key) {
                state.tts_request_id = request_id;
            }
        }
    } else {
        crate::log_info!("[TTS] ERROR: full_text is empty - nothing to speak");
    }

    button_canvas::update_window_position(hwnd);
}

/// Whether the last user-closed overlay batch can be restored.
pub fn can_restore_last_closed() -> bool {
    restore::can_restore_last_closed()
}

/// Cumulative recent restore counts for the tray submenu (up to 5 batches).
pub fn recent_restore_option_counts() -> Vec<usize> {
    restore::recent_restore_option_counts()
}

/// Restore the last user-closed overlay batch.
pub fn restore_last_closed() -> bool {
    restore::restore_last_closed()
}

/// Restore the newest `batch_count` closed batches as one operation.
pub fn restore_recent(batch_count: usize) -> bool {
    restore::restore_recent(batch_count)
}

/// Trigger close for a single window and record it for tray restore.
pub fn trigger_close_window(hwnd: HWND) {
    restore::remember_last_closed(&[hwnd]);

    unsafe {
        if windows::Win32::UI::WindowsAndMessaging::IsWindow(Some(hwnd)).as_bool() {
            let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }
}

/// Trigger close for the window group containing `hwnd` (linked chain BFS).
/// Signals each window's cancellation token to stop streaming, then posts WM_CLOSE.
pub fn trigger_close_group(hwnd: HWND) {
    let group = state::get_window_group(hwnd);
    let group_hwnds: Vec<HWND> = group.iter().map(|(h, _)| *h).collect();

    restore::remember_last_closed(&group_hwnds);

    // Signal all tokens in the group
    {
        let states = WINDOW_STATES.lock().unwrap();
        for (h, _) in &group {
            if let Some(state) = states.get(&(h.0 as isize))
                && let Some(ref token) = state.cancellation_token
            {
                token.cancel();
            }
        }
    }

    for (h, _) in group {
        unsafe {
            if windows::Win32::UI::WindowsAndMessaging::IsWindow(Some(h)).as_bool() {
                let _ = PostMessageW(Some(h), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }
    }
}

/// Trigger close all windows on screen.
/// Signals all cancellation tokens to stop streaming, then posts WM_CLOSE to each window.
pub fn trigger_close_all() {
    let targets: Vec<HWND> = {
        let states = WINDOW_STATES.lock().unwrap();
        for state in states.values() {
            if let Some(ref token) = state.cancellation_token {
                token.cancel();
            }
        }
        states
            .keys()
            .map(|&k| HWND(k as *mut std::ffi::c_void))
            .collect()
    };

    restore::remember_last_closed(&targets);

    for hwnd in targets {
        unsafe {
            if windows::Win32::UI::WindowsAndMessaging::IsWindow(Some(hwnd)).as_bool() {
                let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }
    }
}

/// Trigger drag window (move by delta)
pub fn trigger_drag_window(hwnd: HWND, dx: i32, dy: i32) {
    unsafe {
        let mut rect = windows::Win32::Foundation::RECT::default();
        if windows::Win32::UI::WindowsAndMessaging::GetWindowRect(hwnd, &mut rect).is_ok() {
            let (nx, ny) = (rect.left + dx, rect.top + dy);
            let (nw, nh) = (rect.right - rect.left, rect.bottom - rect.top);

            let _ = windows::Win32::UI::WindowsAndMessaging::SetWindowPos(
                hwnd,
                None,
                nx,
                ny,
                0,
                0,
                windows::Win32::UI::WindowsAndMessaging::SWP_NOSIZE
                    | windows::Win32::UI::WindowsAndMessaging::SWP_NOZORDER
                    | windows::Win32::UI::WindowsAndMessaging::SWP_NOACTIVATE,
            );

            // Update canvas with new position WITHOUT calling GetWindowRect again
            button_canvas::update_window_position_direct(hwnd, nx, ny, nw, nh);
        }
    }
}
