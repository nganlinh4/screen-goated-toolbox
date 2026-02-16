//! IPC message handling for button canvas

use super::{
    get_dpi_scale, send_refine_text_update, update_canvas, ACTIVE_DRAG_SNAPSHOT,
    ACTIVE_DRAG_TARGET, CANVAS_HWND, DRAG_IS_GROUP, IS_DRAGGING_EXTERNAL, LAST_DRAG_POS,
    MARKDOWN_WINDOWS, START_DRAG_POS,
};
use crate::overlay::result::state::WINDOW_STATES;
use std::sync::atomic::Ordering;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;

/// Handle IPC messages from the canvas WebView
pub fn handle_ipc_message(body: &str) {
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(body) {
        let action = json.get("action").and_then(|v| v.as_str()).unwrap_or("");

        // Handle clickable regions update (global, not per-window)
        if action == "update_clickable_regions" {
            handle_update_clickable_regions(&json);
            return;
        }

        let hwnd_str = json.get("hwnd").and_then(|v| v.as_str()).unwrap_or("0");
        let hwnd_val: isize = hwnd_str.parse().unwrap_or(0);

        if hwnd_val == 0 {
            return;
        }

        let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);

        match action {
            "copy" => handle_copy(hwnd),
            "undo" => handle_undo(hwnd),
            "redo" => handle_redo(hwnd),
            "edit" => handle_edit(hwnd),
            "markdown" => crate::overlay::result::trigger_markdown_toggle(hwnd),
            "download" => handle_download(hwnd),
            "back" => handle_back(hwnd),
            "forward" => handle_forward(hwnd),
            "speaker" => handle_speaker(hwnd),
            "broom_click" => handle_broom_click(hwnd),
            "broom_right" => handle_broom_right(hwnd),
            "broom_middle" => crate::overlay::result::trigger_close_all(),
            "broom_drag_start" => handle_broom_drag_start(hwnd, false, false),
            "broom_group_drag_start" => handle_broom_drag_start(hwnd, true, false),
            "broom_all_drag_start" => handle_broom_drag_start(hwnd, true, true),
            "set_opacity" => handle_set_opacity(hwnd, &json),
            "request_update" => update_canvas(),
            "broom_drag" => handle_broom_drag(hwnd, &json),
            "submit_refine" => handle_submit_refine(hwnd, &json),
            "cancel_refine" => crate::overlay::result::trigger_refine_cancel(hwnd),
            "history_up_refine" => handle_history_up(hwnd, &json),
            "history_down_refine" => handle_history_down(hwnd, &json),
            "mic" => handle_mic(),
            "request_focus" => handle_request_focus(),
            _ => {}
        }
    }
}

fn handle_update_clickable_regions(json: &serde_json::Value) {
    if let Some(regions) = json.get("regions").and_then(|v| v.as_array()) {
        let canvas_hwnd = HWND(CANVAS_HWND.load(Ordering::SeqCst) as *mut std::ffi::c_void);
        if canvas_hwnd.0.is_null() {
            return;
        }

        // If currently dragging external window, IGNORE region updates
        if IS_DRAGGING_EXTERNAL.load(Ordering::SeqCst) {
            return;
        }

        unsafe {
            let combined_rgn = CreateRectRgn(0, 0, 0, 0);
            let scale = get_dpi_scale();

            for r in regions {
                let logical_x = r.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let logical_y = r.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let logical_w = r.get("w").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let logical_h = r.get("h").and_then(|v| v.as_f64()).unwrap_or(0.0);

                let x = (logical_x * scale) as i32;
                let y = (logical_y * scale) as i32;
                let w = (logical_w * scale) as i32;
                let h = (logical_h * scale) as i32;

                let rgn = CreateRectRgn(x, y, x + w, y + h);
                let _ = CombineRgn(Some(combined_rgn), Some(combined_rgn), Some(rgn), RGN_OR);
                let _ = DeleteObject(rgn.into());
            }

            let _ = SetWindowRgn(canvas_hwnd, Some(combined_rgn), true);
        }
    }
}

fn handle_copy(hwnd: HWND) {
    unsafe {
        let _ = PostMessageW(
            Some(hwnd),
            super::super::event_handler::misc::WM_COPY_CLICK,
            WPARAM(0),
            LPARAM(0),
        );
    }
}

fn handle_undo(hwnd: HWND) {
    unsafe {
        let _ = PostMessageW(
            Some(hwnd),
            super::super::event_handler::misc::WM_UNDO_CLICK,
            WPARAM(0),
            LPARAM(0),
        );
    }
}

fn handle_redo(hwnd: HWND) {
    unsafe {
        let _ = PostMessageW(
            Some(hwnd),
            super::super::event_handler::misc::WM_REDO_CLICK,
            WPARAM(0),
            LPARAM(0),
        );
    }
}

fn handle_edit(hwnd: HWND) {
    unsafe {
        let _ = PostMessageW(
            Some(hwnd),
            super::super::event_handler::misc::WM_EDIT_CLICK,
            WPARAM(0),
            LPARAM(0),
        );
    }
}

fn handle_download(hwnd: HWND) {
    unsafe {
        let _ = PostMessageW(
            Some(hwnd),
            super::super::event_handler::misc::WM_DOWNLOAD_CLICK,
            WPARAM(0),
            LPARAM(0),
        );
    }
}

fn handle_back(hwnd: HWND) {
    unsafe {
        let _ = PostMessageW(
            Some(hwnd),
            super::super::event_handler::misc::WM_BACK_CLICK,
            WPARAM(0),
            LPARAM(0),
        );
    }
}

fn handle_forward(hwnd: HWND) {
    unsafe {
        let _ = PostMessageW(
            Some(hwnd),
            super::super::event_handler::misc::WM_FORWARD_CLICK,
            WPARAM(0),
            LPARAM(0),
        );
    }
}

fn handle_speaker(hwnd: HWND) {
    crate::log_info!("[TTS] IPC handle_speaker received for hwnd: {:?}", hwnd.0);
    unsafe {
        if let Err(e) = PostMessageW(
            Some(hwnd),
            super::super::event_handler::misc::WM_SPEAKER_CLICK,
            WPARAM(0),
            LPARAM(0),
        ) {
            crate::log_info!(
                "[TTS] ERROR: PostMessageW failed for hwnd: {:?} - {:?}",
                hwnd.0,
                e
            );
        }
    }
}

fn handle_broom_click(hwnd: HWND) {
    unsafe {
        let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
    }
}

fn handle_broom_right(hwnd: HWND) {
    crate::overlay::result::trigger_close_group(hwnd);
}

fn handle_broom_drag_start(hwnd: HWND, is_group: bool, is_all: bool) {
    unsafe {
        use windows::Win32::UI::Input::KeyboardAndMouse::SetCapture;

        let mut pt = POINT::default();
        if GetCursorPos(&mut pt).is_ok() {
            ACTIVE_DRAG_TARGET.store(hwnd.0 as isize, Ordering::SeqCst);
            DRAG_IS_GROUP.store(is_group, Ordering::SeqCst);

            if is_group {
                let mut snapshot = ACTIVE_DRAG_SNAPSHOT.lock().unwrap();
                if is_all {
                    // Collect ALL registered markdown windows
                    let windows = MARKDOWN_WINDOWS.lock().unwrap();
                    *snapshot = windows.keys().cloned().collect();
                } else {
                    // Collect the entire group once at start
                    let group = crate::overlay::result::state::get_window_group(hwnd);
                    *snapshot = group.into_iter().map(|(h, _)| h.0 as isize).collect();
                }
            }

            let mut last = LAST_DRAG_POS.lock().unwrap();
            last.x = pt.x;
            last.y = pt.y;

            let mut start = START_DRAG_POS.lock().unwrap();
            start.x = pt.x;
            start.y = pt.y;

            let canvas_val = CANVAS_HWND.load(Ordering::SeqCst);
            if canvas_val != 0 {
                let canvas_hwnd = HWND(canvas_val as *mut std::ffi::c_void);
                let _ = SetCapture(canvas_hwnd);
                update_canvas();
            }
        }
    }
}

fn handle_set_opacity(hwnd: HWND, json: &serde_json::Value) {
    if let Some(value) = json.get("value").and_then(|v| v.as_f64()) {
        let percent = value as u8;
        let alpha = ((value / 100.0) * 255.0) as u8;

        // Update state so it persists across button canvas refreshes
        {
            let mut states = WINDOW_STATES.lock().unwrap();
            if let Some(state) = states.get_mut(&(hwnd.0 as isize)) {
                state.opacity_percent = percent;
            }
        }

        unsafe {
            use windows::Win32::UI::WindowsAndMessaging::{SetLayeredWindowAttributes, LWA_ALPHA};
            let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), alpha, LWA_ALPHA);
        }
    }
}

fn handle_broom_drag(hwnd: HWND, json: &serde_json::Value) {
    // Legacy JS-driven drag (unused now but kept for compatibility)
    let scale = get_dpi_scale();
    let dx = (json.get("dx").and_then(|v| v.as_f64()).unwrap_or(0.0) * scale).round() as i32;
    let dy = (json.get("dy").and_then(|v| v.as_f64()).unwrap_or(0.0) * scale).round() as i32;
    crate::overlay::result::trigger_drag_window(hwnd, dx, dy);
}

fn handle_submit_refine(hwnd: HWND, json: &serde_json::Value) {
    let text = json.get("text").and_then(|v| v.as_str()).unwrap_or("");
    crate::overlay::result::trigger_refine_submit(hwnd, text);
}

fn handle_history_up(hwnd: HWND, json: &serde_json::Value) {
    let current = json.get("text").and_then(|v| v.as_str()).unwrap_or("");
    if let Some(text) = crate::overlay::input_history::navigate_history_up(current) {
        send_refine_text_update(hwnd, &text, false);
    }
}

fn handle_history_down(hwnd: HWND, json: &serde_json::Value) {
    let current = json.get("text").and_then(|v| v.as_str()).unwrap_or("");
    if let Some(text) = crate::overlay::input_history::navigate_history_down(current) {
        send_refine_text_update(hwnd, &text, false);
    }
}

fn handle_mic() {
    let transcribe_idx = {
        let app = crate::APP.lock().unwrap();
        app.config
            .presets
            .iter()
            .position(|p| p.id == "preset_transcribe")
    };

    if let Some(preset_idx) = transcribe_idx {
        std::thread::spawn(move || {
            crate::overlay::recording::show_recording_overlay(preset_idx);
        });
    }
}

fn handle_request_focus() {
    unsafe {
        let canvas_val = CANVAS_HWND.load(Ordering::SeqCst);
        if canvas_val != 0 {
            let canvas_hwnd = HWND(canvas_val as *mut _);
            let _ = SetForegroundWindow(canvas_hwnd);
        }
    }
}
