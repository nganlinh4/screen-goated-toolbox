//! IPC message handling for markdown view

use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::overlay::result::event_handler::misc::{WM_BROOM_DRAG_START, WM_COPY_CLICK};

/// Handle IPC messages from markdown WebView
pub fn handle_markdown_ipc(hwnd: HWND, msg: &str) {
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(msg)
        && let Some(action) = json.get("action").and_then(|s| s.as_str())
    {
        match action {
            "copy" => {
                post_result_window_message(hwnd, WM_COPY_CLICK);
            }
            "close" | "broom_click" => {
                post_result_window_message(hwnd, WM_CLOSE);
            }
            "broom_drag_start" => {
                post_result_window_message(hwnd, WM_BROOM_DRAG_START);
            }
            "fit_debug" => {
                crate::log_info!("[MarkdownFitDebug] hwnd={:?} payload={}", hwnd, json);
            }
            "render_diagnostics" => {
                crate::log_info!("[MarkdownDiag] hwnd={:?} payload={}", hwnd, json);
            }
            _ => {}
        }
    }
}

fn post_result_window_message(hwnd: HWND, msg: u32) {
    unsafe {
        if IsWindow(Some(hwnd)).as_bool() {
            let _ = PostMessageW(Some(hwnd), msg, WPARAM(0), LPARAM(0));
        }
    }
}
