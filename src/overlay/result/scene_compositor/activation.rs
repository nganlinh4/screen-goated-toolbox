use super::child::CARDS;
use super::protocol::SceneRect;
use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, GWL_EXSTYLE, GetCursorPos, GetForegroundWindow, GetSystemMetrics,
    GetWindowLongPtrW, GetWindowThreadProcessId, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
    SetForegroundWindow, SetWindowLongPtrW, WS_EX_NOACTIVATE,
};

pub(super) fn focus_renderer(hwnd: HWND) {
    make_renderer_activatable(hwnd);
    bring_renderer_to_foreground(hwnd);
    super::child::focus_webview();
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

pub(super) fn restore_nonactivating_style(hwnd: HWND) {
    unsafe {
        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let _ = SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style | WS_EX_NOACTIVATE.0 as isize);
    }
}

pub(super) fn cursor_is_over_result_card() -> bool {
    let mut cursor = POINT::default();
    if unsafe { GetCursorPos(&mut cursor) }.is_err() {
        return false;
    }
    let x = cursor.x - unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
    let y = cursor.y - unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
    CARDS
        .lock()
        .unwrap()
        .values()
        .any(|card| card.visible && scene_rect_contains(&card.rect, x, y))
}

fn scene_rect_contains(rect: &SceneRect, x: i32, y: i32) -> bool {
    rect.width > 0
        && rect.height > 0
        && x >= rect.x
        && y >= rect.y
        && x < rect.x.saturating_add(rect.width)
        && y < rect.y.saturating_add(rect.height)
}

#[cfg(test)]
mod tests {
    use super::{SceneRect, scene_rect_contains};

    #[test]
    fn activation_hit_test_uses_only_the_result_card_surface() {
        let rect = SceneRect {
            x: 100,
            y: 50,
            width: 300,
            height: 200,
        };

        assert!(scene_rect_contains(&rect, 100, 50));
        assert!(scene_rect_contains(&rect, 399, 249));
        assert!(!scene_rect_contains(&rect, 400, 249));
        assert!(!scene_rect_contains(&rect, 399, 250));
        assert!(!scene_rect_contains(&SceneRect::default(), 0, 0));
    }
}
