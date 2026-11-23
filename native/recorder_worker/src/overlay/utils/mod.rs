#[path = "../../../../../src/overlay/utils/script_json.rs"]
mod script_json;

pub use script_json::inline_script_json;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
use windows::Win32::UI::WindowsAndMessaging::{HTCAPTION, SendMessageW, WM_NCLBUTTONDOWN};

pub fn begin_window_drag(hwnd: HWND) {
    unsafe {
        let _ = ReleaseCapture();
        let _ = SendMessageW(
            hwnd,
            WM_NCLBUTTONDOWN,
            Some(WPARAM(HTCAPTION as usize)),
            Some(LPARAM(0)),
        );
    }
}

pub fn show_api_key_error_notification(error: &str, language: &str) {
    if let Some(message) = crate::credential_error::message(error, language) {
        crate::overlay::auto_copy_badge::show_error_notification(&message);
    }
}
