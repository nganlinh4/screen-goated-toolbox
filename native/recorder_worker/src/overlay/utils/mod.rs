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
    let provider = if error.contains("google") || error.contains("gemini") {
        "Google Gemini"
    } else {
        "API"
    };
    let message = if error.contains("NO_API_KEY") {
        match language {
            "vi" => format!("Bạn chưa nhập {provider} API key!"),
            "ko" => format!("{provider} API 키를 입력하지 않았습니다!"),
            _ => format!("You haven't entered a {provider} API key!"),
        }
    } else if error.contains("INVALID_API_KEY") {
        match language {
            "vi" => format!("{provider} API key không hợp lệ!"),
            "ko" => format!("{provider} API 키가 유효하지 않습니다!"),
            _ => format!("Invalid {provider} API key!"),
        }
    } else {
        return;
    };
    crate::overlay::auto_copy_badge::show_error_notification(&message);
}
