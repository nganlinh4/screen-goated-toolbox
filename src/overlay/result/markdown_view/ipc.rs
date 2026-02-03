//! IPC message handling for markdown view

use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

/// Handle IPC messages from markdown WebView
pub fn handle_markdown_ipc(hwnd: HWND, msg: &str) {
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(msg) {
        if let Some(action) = json.get("action").and_then(|s| s.as_str()) {
            match action {
                "copy" => {
                    crate::overlay::result::trigger_copy(hwnd);
                }
                "close" | "broom_click" => unsafe {
                    let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                },
                "broom_drag_start" => {
                    unsafe {
                        // Native Window Drag
                        // ReleaseCapture required before SC_MOVE
                        use windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
                        let _ = ReleaseCapture();
                        let _ = PostMessageW(
                            Some(hwnd),
                            WM_SYSCOMMAND,
                            WPARAM(0xF012), // SC_MOVE (0xF010) + HTCAPTION (2)
                            LPARAM(0),
                        );
                    }
                }
                _ => {}
            }
        }
    }
}
