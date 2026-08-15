// --- WINDOW RESIZE WNDPROC HOOK ---
// Supplies native resize hit testing and exposes the modal sizing lifecycle to
// the renderer. Rendering remains owned by eframe/wgpu.

use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    CallWindowProcW, DefWindowProcW, GWL_STYLE, GWLP_WNDPROC, GetWindowLongPtrW, GetWindowLongW,
    GetWindowRect, HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT, HTCLIENT, HTLEFT, HTNOWHERE, HTRIGHT,
    HTTOP, HTTOPLEFT, HTTOPRIGHT, SetWindowLongPtrW, WM_ENTERSIZEMOVE, WM_EXITSIZEMOVE,
    WM_NCHITTEST, WM_SIZING, WS_MAXIMIZE,
};

const RESIZE_BORDER_DIP: i32 = 6;

type RawWndProcFn = unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT;

static OLD_WNDPROC: AtomicIsize = AtomicIsize::new(0);
static LIVE_RESIZE: AtomicBool = AtomicBool::new(false);

pub fn is_live_resize() -> bool {
    LIVE_RESIZE.load(Ordering::Relaxed)
}

fn next_live_resize_state(current: bool, message: u32) -> bool {
    match message {
        WM_ENTERSIZEMOVE | WM_EXITSIZEMOVE => false,
        WM_SIZING => true,
        _ => current,
    }
}

#[inline]
unsafe fn call_old(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        let old = OLD_WNDPROC.load(Ordering::SeqCst);
        if old != 0 {
            let function: RawWndProcFn = std::mem::transmute(old as usize);
            CallWindowProcW(Some(function), hwnd, msg, wparam, lparam)
        } else {
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
    }
}

unsafe extern "system" fn resize_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        let resize_state = next_live_resize_state(is_live_resize(), msg);
        if resize_state != is_live_resize() {
            LIVE_RESIZE.store(resize_state, Ordering::Relaxed);
        }

        if msg == WM_NCHITTEST {
            let default = call_old(hwnd, msg, wparam, lparam);
            if default.0 != HTCLIENT as isize && default.0 != HTNOWHERE as isize {
                return default;
            }

            let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
            if style & WS_MAXIMIZE.0 != 0 {
                return default;
            }

            let x = (lparam.0 & 0xFFFF) as i16 as i32;
            let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
            let mut rect = RECT::default();
            if GetWindowRect(hwnd, &mut rect).is_err() {
                return default;
            }

            let dpi = GetDpiForWindow(hwnd);
            let border = (RESIZE_BORDER_DIP * dpi as i32 + 48) / 96;
            let on_left = x < rect.left + border;
            let on_right = x >= rect.right - border;
            let on_top = y < rect.top + border;
            let on_bottom = y >= rect.bottom - border;

            let hit = match (on_left, on_right, on_top, on_bottom) {
                (true, _, true, _) => HTTOPLEFT as isize,
                (_, true, true, _) => HTTOPRIGHT as isize,
                (true, _, _, true) => HTBOTTOMLEFT as isize,
                (_, true, _, true) => HTBOTTOMRIGHT as isize,
                (true, false, false, false) => HTLEFT as isize,
                (false, true, false, false) => HTRIGHT as isize,
                (false, false, true, false) => HTTOP as isize,
                (false, false, false, true) => HTBOTTOM as isize,
                _ => return default,
            };
            return LRESULT(hit);
        }

        call_old(hwnd, msg, wparam, lparam)
    }
}

/// Hook the main HWND to add resize hit-testing. Repeated calls are no-ops.
pub fn install(hwnd: HWND) {
    if OLD_WNDPROC.load(Ordering::SeqCst) != 0 {
        return;
    }
    unsafe {
        let current = GetWindowLongPtrW(hwnd, GWLP_WNDPROC);
        if current == 0 {
            return;
        }
        let old = SetWindowLongPtrW(hwnd, GWLP_WNDPROC, resize_wndproc as *const () as isize);
        OLD_WNDPROC.store(old, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_sizing_messages_enable_the_render_fast_path() {
        assert!(!next_live_resize_state(true, WM_ENTERSIZEMOVE));
        assert!(next_live_resize_state(false, WM_SIZING));
        assert!(next_live_resize_state(true, WM_NCHITTEST));
        assert!(!next_live_resize_state(true, WM_EXITSIZEMOVE));
    }
}
