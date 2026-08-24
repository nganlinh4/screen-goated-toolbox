pub mod button_canvas;
mod event_handler;
pub(crate) mod latency;
pub mod layout;
pub mod markdown_view;
mod raw_webview;
mod refine;
mod restore;
pub mod scene_compositor;
pub(crate) mod smoke;
pub mod state;
mod window;

pub use refine::{trigger_edit, trigger_refine_cancel, trigger_refine_submit};
pub use state::{
    ChainCancelToken, RefineContext, ResultControlOptions, ResultPresentation,
    SourceReplacementRegion, WINDOW_STATES, WindowType, close_chain_windows, link_windows,
};
pub use window::{
    ResultWindowParams, TextOnlyResultOptions, create_result_window,
    create_text_only_result_window, get_chain_color, update_text_only_segments, update_window_text,
};
pub(crate) use window::{
    configure_text_only_result_window, create_result_window_shell, initialize_result_window,
};

pub(crate) fn subtle_outline_color(is_dark: bool) -> &'static str {
    if is_dark {
        "rgba(255, 255, 255, 0.1)"
    } else {
        "rgba(0, 0, 0, 0.08)"
    }
}

pub fn update_theme(is_dark: bool) {
    scene_compositor::update_theme(is_dark);
}

pub fn raise_window(hwnd: HWND) {
    raw_webview::raise_window(hwnd);
    scene_compositor::raise_window(hwnd);
}

// Result-control actions routed back from the compositor process.
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
    // Only update internal state if overwriting (for consistency)
    if !is_insert {
        let hwnd_key = hwnd.0 as isize;
        let mut states = WINDOW_STATES.lock().unwrap();
        if let Some(state) = states.get_mut(&hwnd_key) {
            state.input_text = text.to_string();
        }
    }
    button_canvas::send_refine_text_update(hwnd, text, is_insert);
}

/// Trigger copy action on a result window
pub fn trigger_copy(hwnd: HWND) {
    let hwnd_key = hwnd.0 as isize;

    let (text, group_actions) = control_action_text(hwnd);

    if !text.is_empty() {
        crate::overlay::utils::copy_to_clipboard(&text, hwnd);
        if group_actions {
            crate::overlay::auto_copy_badge::show_auto_copy_badge_text(&text);
        }

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

    let prev_text = {
        let mut states = WINDOW_STATES.lock().unwrap();
        if let Some(state) = states.get_mut(&hwnd_key) {
            if let Some(last) = state.text_history.pop() {
                let current = state.full_text.clone();
                state.redo_history.push(current);
                state.full_text = last.clone();
                Some(last)
            } else {
                None
            }
        } else {
            None
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

        scene_compositor::sync_window(hwnd, unsafe {
            windows::Win32::UI::WindowsAndMessaging::IsWindowVisible(hwnd).as_bool()
        });
        raw_webview::request_sync(hwnd);

        // Update canvas
        button_canvas::update_window_position(hwnd);
    }
}

/// Trigger redo action on a result window
pub fn trigger_redo(hwnd: HWND) {
    let hwnd_key = hwnd.0 as isize;

    let next_text = {
        let mut states = WINDOW_STATES.lock().unwrap();
        if let Some(state) = states.get_mut(&hwnd_key) {
            if let Some(redo) = state.redo_history.pop() {
                let current = state.full_text.clone();
                state.text_history.push(current);
                state.full_text = redo.clone();
                Some(redo)
            } else {
                None
            }
        } else {
            None
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

        scene_compositor::sync_window(hwnd, unsafe {
            windows::Win32::UI::WindowsAndMessaging::IsWindowVisible(hwnd).as_bool()
        });
        raw_webview::request_sync(hwnd);

        button_canvas::update_window_position(hwnd);
    }
}

/// Trigger speaker/TTS
pub fn trigger_speaker(hwnd: HWND) {
    let hwnd_key = hwnd.0 as isize;
    crate::log_info!("[TTS] trigger_speaker called for hwnd: {}", hwnd_key);

    let (full_text, _) = control_action_text(hwnd);
    let (current_tts_id, is_loading, state_exists) = {
        let states = WINDOW_STATES.lock().unwrap();
        if let Some(s) = states.get(&hwnd_key) {
            (s.tts_request_id, s.tts_loading, true)
        } else {
            (0, false, false)
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

pub(crate) fn control_action_text(hwnd: HWND) -> (String, bool) {
    let hwnd_key = hwnd.0 as isize;
    let group_actions = WINDOW_STATES
        .lock()
        .unwrap()
        .get(&hwnd_key)
        .and_then(|state| state.control_options.as_ref())
        .is_some_and(|options| options.group_actions);
    if !group_actions {
        let text = WINDOW_STATES
            .lock()
            .unwrap()
            .get(&hwnd_key)
            .map(|state| state.full_text.clone())
            .unwrap_or_default();
        return (text, false);
    }

    let mut group = state::get_window_group(hwnd);
    group.sort_by_key(|(_, rect)| (rect.top, rect.left));
    let states = WINDOW_STATES.lock().unwrap();
    let text = group
        .into_iter()
        .filter_map(|(target, _)| states.get(&(target.0 as isize)))
        .map(|state| state.full_text.trim())
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\r\n");
    (text, true)
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
