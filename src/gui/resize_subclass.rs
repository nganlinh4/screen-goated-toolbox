// --- WINDOW RESIZE WNDPROC HOOK ---
// Replaces the eframe WndProc with a thin wrapper that returns proper HT* codes
// from WM_NCHITTEST for the resize border area.  This lets Windows start the
// synchronous sizing modal loop on WM_LBUTTONDOWN, bypassing eframe/winit's
// async PostMessage(WM_SYSCOMMAND, SC_SIZE) approach which fails on this
// transparent-borderless window configuration.

use std::sync::atomic::{AtomicIsize, Ordering};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    CallWindowProcW, DefWindowProcW, GetWindowLongPtrW, GetWindowLongW, GetWindowRect, GWL_STYLE,
    GWLP_WNDPROC, HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT, HTCLIENT, HTLEFT, HTNOWHERE, HTRIGHT,
    HTTOP, HTTOPLEFT, HTTOPRIGHT, SetWindowLongPtrW, WM_NCHITTEST, WS_MAXIMIZE,
};

/// Logical resize border width in pixels at 96 DPI.
const RESIZE_BORDER_DIP: i32 = 6;

type RawWndProcFn = unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT;

/// Stores the previous WndProc so we can forward unhandled messages.
static OLD_WNDPROC: AtomicIsize = AtomicIsize::new(0);

#[inline]
unsafe fn call_old(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let old = OLD_WNDPROC.load(Ordering::SeqCst);
    if old != 0 {
        let f: RawWndProcFn = std::mem::transmute(old as usize);
        CallWindowProcW(Some(f), hwnd, msg, wparam, lparam)
    } else {
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}

unsafe extern "system" fn resize_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_NCHITTEST {
        let default = call_old(hwnd, msg, wparam, lparam);

        // Only override HTCLIENT / HTNOWHERE — leave caption/existing codes alone.
        if default.0 != HTCLIENT as isize && default.0 != HTNOWHERE as isize {
            return default;
        }

        // No resize border when maximized.
        let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
        if style & WS_MAXIMIZE.0 != 0 {
            return default;
        }

        // Extract signed screen coordinates (critical for negative coords on
        // left/top monitors in multi-monitor setups).
        let x = (lparam.0 & 0xFFFF) as i16 as i32;
        let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;

        let mut rect = RECT::default();
        if GetWindowRect(hwnd, &mut rect).is_err() {
            return default;
        }

        // Scale border for this window's DPI.
        let dpi = GetDpiForWindow(hwnd);
        let border = (RESIZE_BORDER_DIP * dpi as i32 + 48) / 96; // round to nearest px

        let on_left = x < rect.left + border;
        let on_right = x >= rect.right - border;
        let on_top = y < rect.top + border;
        let on_bottom = y >= rect.bottom - border;

        let ht = match (on_left, on_right, on_top, on_bottom) {
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
        return LRESULT(ht);
    }

    call_old(hwnd, msg, wparam, lparam)
}

/// Hook the WndProc of the given HWND to add resize hit-testing.
/// Safe to call multiple times — subsequent calls are no-ops.
pub fn install(hwnd: HWND) {
    // Only install once.
    if OLD_WNDPROC.load(Ordering::SeqCst) != 0 {
        return;
    }
    unsafe {
        // Read the current proc before replacing it.
        let current = GetWindowLongPtrW(hwnd, GWLP_WNDPROC);
        if current == 0 {
            return;
        }
        let old = SetWindowLongPtrW(hwnd, GWLP_WNDPROC, resize_wndproc as *const () as isize);
        OLD_WNDPROC.store(old, Ordering::SeqCst);
    }
}
