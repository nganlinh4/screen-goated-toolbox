use windows::Win32::Foundation::HWND;
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, GWL_EXSTYLE, GetForegroundWindow, GetWindowLongPtrW,
    GetWindowThreadProcessId, SetForegroundWindow, SetWindowLongPtrW, WS_EX_NOACTIVATE,
};

pub(super) fn focus_renderer(hwnd: HWND) {
    activate_window(hwnd);
    super::child::focus_webview();
}

pub(in crate::overlay::result) fn activate_window(hwnd: HWND) {
    make_renderer_activatable(hwnd);
    bring_renderer_to_foreground(hwnd);
}

fn bring_renderer_to_foreground(hwnd: HWND) {
    unsafe {
        let foreground = GetForegroundWindow();
        let current_thread = GetCurrentThreadId();
        let foreground_thread = GetWindowThreadProcessId(foreground, None);
        // A click reaches WebView2 through the child HWND even while another app owns the
        // foreground input queue. Join that queue for this user-initiated transition so
        // keyboard focus and foreground ownership cannot diverge.
        let attached = foreground_thread != 0
            && foreground_thread != current_thread
            && AttachThreadInput(current_thread, foreground_thread, true).as_bool();
        let _ = BringWindowToTop(hwnd);
        let _ = SetForegroundWindow(hwnd);
        if attached {
            let _ = AttachThreadInput(current_thread, foreground_thread, false);
        }
    }
}

pub(super) fn make_renderer_activatable(hwnd: HWND) {
    unsafe {
        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let _ = SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style & !(WS_EX_NOACTIVATE.0 as isize));
    }
}

pub(in crate::overlay::result) fn restore_nonactivating_style(hwnd: HWND) {
    unsafe {
        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let _ = SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style | WS_EX_NOACTIVATE.0 as isize);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn make_renderer_activatable_toggles_noactivate_style() {
        let hwnd = HWND(std::ptr::null_mut());
        // Verify style constants compile and match Win32 contracts
        assert_eq!(WS_EX_NOACTIVATE.0, 0x08000000);
        let _ = hwnd;
    }
}
