// --- WINDOW RESIZE WNDPROC HOOK ---
// Replaces the eframe WndProc with a thin wrapper that returns proper HT* codes
// from WM_NCHITTEST for the resize border area.  This lets Windows start the
// synchronous sizing modal loop on WM_LBUTTONDOWN, bypassing eframe/winit's
// async PostMessage(WM_SYSCOMMAND, SC_SIZE) approach which fails on this
// undecorated window configuration.

use std::sync::{
    LazyLock, Mutex,
    atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering},
};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BitBlt, ClientToScreen, CreateCompatibleBitmap, CreateCompatibleDC, CreateSolidBrush, DeleteDC,
    DeleteObject, FillRect, GetDC, HBITMAP, HDC, HGDIOBJ, ReleaseDC, SRCCOPY, SelectObject,
};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    CallWindowProcW, DefWindowProcW, GWL_STYLE, GWLP_WNDPROC, GetClientRect, GetWindowLongPtrW,
    GetWindowLongW, GetWindowRect, HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT, HTCLIENT, HTLEFT,
    HTNOWHERE, HTRIGHT, HTTOP, HTTOPLEFT, HTTOPRIGHT, SetWindowLongPtrW, WM_ENTERSIZEMOVE,
    WM_EXITSIZEMOVE, WM_NCHITTEST, WM_SIZE, WM_SIZING, WMSZ_BOTTOMLEFT, WMSZ_LEFT, WMSZ_TOP,
    WMSZ_TOPLEFT, WMSZ_TOPRIGHT, WS_MAXIMIZE,
};

/// Logical resize border width in pixels at 96 DPI.
const RESIZE_BORDER_DIP: i32 = 6;

type RawWndProcFn = unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT;

/// Stores the previous WndProc so we can forward unhandled messages.
static OLD_WNDPROC: AtomicIsize = AtomicIsize::new(0);
// COLORREF stores bytes as 0x00BBGGRR. This default matches the dark panel
// closely until the first egui frame publishes the active theme color.
static BACKGROUND_COLORREF: AtomicU32 = AtomicU32::new(0x0022_1D1C);
static RESIZE_ACTIVE: AtomicBool = AtomicBool::new(false);
static RESIZE_BACKING: LazyLock<Mutex<Option<ResizeBacking>>> = LazyLock::new(|| Mutex::new(None));

struct ResizeBacking {
    memory_dc: HDC,
    bitmap: HBITMAP,
    previous_bitmap: HGDIOBJ,
    width: i32,
    height: i32,
    sizing_edge: u32,
}

// The backing is created, used, and destroyed by the main window thread. The mutex exists only
// to give the process-wide WndProc state safe ownership and poison recovery.
unsafe impl Send for ResizeBacking {}

impl Drop for ResizeBacking {
    fn drop(&mut self) {
        unsafe {
            let _ = SelectObject(self.memory_dc, self.previous_bitmap);
            let _ = DeleteObject(self.bitmap.into());
            let _ = DeleteDC(self.memory_dc);
        }
    }
}

pub fn set_background_color(color: eframe::egui::Color32) {
    let colorref =
        u32::from(color.r()) | (u32::from(color.g()) << 8) | (u32::from(color.b()) << 16);
    BACKGROUND_COLORREF.store(colorref, Ordering::Relaxed);
}

fn backing_origin(
    sizing_edge: u32,
    client_width: i32,
    client_height: i32,
    backing_width: i32,
    backing_height: i32,
) -> (i32, i32) {
    let x = if matches!(sizing_edge, WMSZ_LEFT | WMSZ_TOPLEFT | WMSZ_BOTTOMLEFT) {
        client_width - backing_width
    } else {
        0
    };
    let y = if matches!(sizing_edge, WMSZ_TOP | WMSZ_TOPLEFT | WMSZ_TOPRIGHT) {
        client_height - backing_height
    } else {
        0
    };
    (x, y)
}

fn resize_backing() -> std::sync::MutexGuard<'static, Option<ResizeBacking>> {
    RESIZE_BACKING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

unsafe fn capture_resize_backing(hwnd: HWND, sizing_edge: u32) -> Option<ResizeBacking> {
    unsafe {
        let mut client = RECT::default();
        if GetClientRect(hwnd, &mut client).is_err() {
            return None;
        }
        let width = client.right - client.left;
        let height = client.bottom - client.top;
        if width <= 0 || height <= 0 {
            return None;
        }

        let mut origin = POINT::default();
        if !ClientToScreen(hwnd, &mut origin).as_bool() {
            return None;
        }

        // Capture from the composed desktop rather than the window DC: flip-model DirectX
        // surfaces can return black when copied directly through GDI.
        let screen_dc = GetDC(None);
        if screen_dc.is_invalid() {
            return None;
        }
        let memory_dc = CreateCompatibleDC(Some(screen_dc));
        if memory_dc.is_invalid() {
            let _ = ReleaseDC(None, screen_dc);
            return None;
        }
        let bitmap = CreateCompatibleBitmap(screen_dc, width, height);
        if bitmap.is_invalid() {
            let _ = DeleteDC(memory_dc);
            let _ = ReleaseDC(None, screen_dc);
            return None;
        }
        let previous_bitmap = SelectObject(memory_dc, bitmap.into());
        let captured = BitBlt(
            memory_dc,
            0,
            0,
            width,
            height,
            Some(screen_dc),
            origin.x,
            origin.y,
            SRCCOPY,
        )
        .is_ok();
        let _ = ReleaseDC(None, screen_dc);
        if !captured {
            let _ = SelectObject(memory_dc, previous_bitmap);
            let _ = DeleteObject(bitmap.into());
            let _ = DeleteDC(memory_dc);
            return None;
        }

        Some(ResizeBacking {
            memory_dc,
            bitmap,
            previous_bitmap,
            width,
            height,
            sizing_edge,
        })
    }
}

unsafe fn paint_resize_backing(hwnd: HWND, width: u32, height: u32) {
    let width = width as i32;
    let height = height as i32;
    if width <= 0 || height <= 0 {
        return;
    }

    unsafe {
        let dc = GetDC(Some(hwnd));
        if dc.is_invalid() {
            return;
        }
        let brush = CreateSolidBrush(COLORREF(BACKGROUND_COLORREF.load(Ordering::Relaxed)));
        if !brush.is_invalid() {
            let client = RECT {
                left: 0,
                top: 0,
                right: width,
                bottom: height,
            };
            FillRect(dc, &client, brush);
            let _ = DeleteObject(brush.into());
        }

        if let Some(backing) = resize_backing().as_ref() {
            let (x, y) = backing_origin(
                backing.sizing_edge,
                width,
                height,
                backing.width,
                backing.height,
            );
            let _ = BitBlt(
                dc,
                x,
                y,
                backing.width,
                backing.height,
                Some(backing.memory_dc),
                0,
                0,
                SRCCOPY,
            );
        }
        let _ = ReleaseDC(Some(hwnd), dc);
    }
}

#[inline]
unsafe fn call_old(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        let old = OLD_WNDPROC.load(Ordering::SeqCst);
        if old != 0 {
            let f: RawWndProcFn = std::mem::transmute(old as usize);
            CallWindowProcW(Some(f), hwnd, msg, wparam, lparam)
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
        if msg == WM_ENTERSIZEMOVE {
            RESIZE_ACTIVE.store(false, Ordering::Relaxed);
            *resize_backing() = None;
        } else if msg == WM_SIZING && !RESIZE_ACTIVE.swap(true, Ordering::Relaxed) {
            *resize_backing() = capture_resize_backing(hwnd, wparam.0 as u32);
        } else if msg == WM_EXITSIZEMOVE {
            let result = call_old(hwnd, msg, wparam, lparam);
            RESIZE_ACTIVE.store(false, Ordering::Relaxed);
            *resize_backing() = None;
            return result;
        }

        if msg == WM_SIZE {
            let packed = lparam.0 as u32;
            let width = packed & 0xffff;
            let height = packed >> 16;
            if RESIZE_ACTIVE.load(Ordering::Relaxed) {
                paint_resize_backing(hwnd, width, height);
            }
        }

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

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::UI::WindowsAndMessaging::WMSZ_BOTTOM;

    #[test]
    fn backing_tracks_the_edge_opposite_the_active_resize_handle() {
        assert_eq!(backing_origin(WMSZ_LEFT, 1_200, 800, 1_000, 700), (200, 0));
        assert_eq!(backing_origin(WMSZ_TOP, 1_200, 800, 1_000, 700), (0, 100));
        assert_eq!(
            backing_origin(WMSZ_TOPLEFT, 1_200, 800, 1_000, 700),
            (200, 100)
        );
        assert_eq!(backing_origin(WMSZ_BOTTOM, 1_200, 800, 1_000, 700), (0, 0));
    }
}
