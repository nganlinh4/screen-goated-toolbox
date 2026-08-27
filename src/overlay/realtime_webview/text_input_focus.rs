//! Scoped activation for real keyboard and IME input inside the compositor WebView.

use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};

use windows::Win32::Foundation::HWND;
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, GWL_EXSTYLE, GetForegroundWindow, GetWindowLongPtrW,
    GetWindowThreadProcessId, IsWindow, SetForegroundWindow, SetWindowLongPtrW, WS_EX_NOACTIVATE,
};

use super::layout::CardRole;

static ACTIVE: AtomicBool = AtomicBool::new(false);
static PREVIOUS_FOREGROUND: AtomicIsize = AtomicIsize::new(0);

pub(super) fn begin(hwnd: HWND, role: CardRole) {
    unsafe {
        if !ACTIVE.swap(true, Ordering::SeqCst) {
            let foreground = GetForegroundWindow();
            if foreground != hwnd {
                PREVIOUS_FOREGROUND.store(foreground.0 as isize, Ordering::SeqCst);
            }
        }

        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let _ = SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style & !(WS_EX_NOACTIVATE.0 as isize));

        let foreground = GetForegroundWindow();
        let current_thread = GetCurrentThreadId();
        let foreground_thread = GetWindowThreadProcessId(foreground, None);
        let attached = foreground_thread != 0
            && foreground_thread != current_thread
            && AttachThreadInput(current_thread, foreground_thread, true).as_bool();
        let _ = BringWindowToTop(hwnd);
        let _ = SetForegroundWindow(hwnd);
        if attached {
            let _ = AttachThreadInput(current_thread, foreground_thread, false);
        }
    }
    super::webview::focus_card_text_input(role);
}

pub(super) fn end(hwnd: HWND) {
    if !ACTIVE.swap(false, Ordering::SeqCst) {
        return;
    }
    unsafe {
        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let _ = SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style | WS_EX_NOACTIVATE.0 as isize);

        let previous = PREVIOUS_FOREGROUND.swap(0, Ordering::SeqCst);
        if previous != 0 {
            let previous = HWND(previous as *mut std::ffi::c_void);
            if previous != hwnd && IsWindow(Some(previous)).as_bool() {
                let _ = SetForegroundWindow(previous);
            }
        }
    }
}

pub(super) fn is_active() -> bool {
    ACTIVE.load(Ordering::SeqCst)
}
