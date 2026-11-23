//! Guard for APIs that send synchronous messages to another app's UI thread.

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    SMTO_ABORTIFHUNG, SMTO_BLOCK, SendMessageTimeoutW, WM_NULL,
};

const RESPONSE_BUDGET_MS: u32 = 250;

/// Whether the window's UI thread answers a no-op message within a short
/// budget. `PrintWindow` sends a synchronous `WM_PRINT` with no timeout, so a
/// window that fails this check must not be printed: the caller would block
/// forever while holding a full-size bitmap and its DCs.
pub(crate) fn window_answers_messages(hwnd: HWND) -> bool {
    let mut result = 0usize;
    unsafe {
        SendMessageTimeoutW(
            hwnd,
            WM_NULL,
            WPARAM(0),
            LPARAM(0),
            SMTO_ABORTIFHUNG | SMTO_BLOCK,
            RESPONSE_BUDGET_MS,
            Some(&mut result),
        ) != LRESULT(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_window_is_not_treated_as_responsive() {
        assert!(!window_answers_messages(HWND(std::ptr::null_mut())));
    }
}
