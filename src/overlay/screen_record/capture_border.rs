// --- CAPTURE BORDER OVERLAY ---
// Draws a colored border around the target window during window capture recording.
// Uses a WS_EX_LAYERED window with LWA_COLORKEY so only the border is visible.
// Display capture uses the system yellow border (DrawBorderSettings::Default).
// Window capture uses this custom blue overlay for a distinct visual indicator.

use std::sync::Once;
use std::sync::atomic::{AtomicIsize, AtomicU64, Ordering};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::{DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateSolidBrush, DeleteObject, EndPaint, FillRect, HBRUSH, InvalidateRect,
    PAINTSTRUCT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;

// Border color: brand blue #60A5FA — RGB(96, 165, 250) → Windows BGR: 0x00_FA_A5_60
const BORDER_COLOR: COLORREF = COLORREF(0x00_FA_A5_60);
// Key color used as the transparent interior. Near-black but not pure black.
// RGB(2, 3, 4) → Windows BGR: 0x00_04_03_02
const KEY_COLOR: COLORREF = COLORREF(0x00_04_03_02);
const BORDER_PX: i32 = 3;

const WM_APP_MOVE_BORDER: u32 = WM_USER + 201;
const WM_APP_SET_BORDER_VISIBLE: u32 = WM_USER + 202;
const BORDER_TRACK_POLL_MS: u64 = 16;

static REGISTER_BORDER_CLASS: Once = Once::new();
static BORDER_HWND: AtomicIsize = AtomicIsize::new(0);
static BORDER_SESSION: AtomicU64 = AtomicU64::new(0);

fn pack_i32_pair(high: i32, low: i32) -> usize {
    (((high as u32 as u64) << 32) | (low as u32 as u64)) as usize
}

fn decode_i32_pair(value: usize) -> (i32, i32) {
    let packed = value as u64;
    ((packed >> 32) as u32 as i32, packed as u32 as i32)
}

fn get_window_bounds(hwnd: HWND) -> Option<RECT> {
    unsafe {
        if !IsWindow(Some(hwnd)).as_bool() {
            return None;
        }

        let mut rect = RECT::default();
        if DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut rect as *mut _ as *mut std::ffi::c_void,
            std::mem::size_of::<RECT>() as u32,
        )
        .is_err()
        {
            let _ = GetWindowRect(hwnd, &mut rect);
        }

        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        if width <= 0 || height <= 0 {
            return None;
        }

        Some(rect)
    }
}

fn post_border_bounds(border_hwnd: HWND, rect: RECT) {
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    unsafe {
        let _ = PostMessageW(
            Some(border_hwnd),
            WM_APP_MOVE_BORDER,
            WPARAM(pack_i32_pair(rect.left, rect.top)),
            LPARAM(pack_i32_pair(width, height) as isize),
        );
    }
}

fn post_border_visibility(border_hwnd: HWND, visible: bool) {
    unsafe {
        let _ = PostMessageW(
            Some(border_hwnd),
            WM_APP_SET_BORDER_VISIBLE,
            WPARAM(usize::from(visible)),
            LPARAM(0),
        );
    }
}

unsafe extern "system" fn border_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        match msg {
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                let hdc = BeginPaint(hwnd, &mut ps);
                let mut rc = RECT::default();
                let _ = GetClientRect(hwnd, &mut rc);
                let w = rc.right - rc.left;
                let h = rc.bottom - rc.top;

                // Fill interior with key color (transparent via LWA_COLORKEY).
                let key_brush = CreateSolidBrush(KEY_COLOR);
                FillRect(hdc, &rc, key_brush);
                let _ = DeleteObject(key_brush.into());

                // Paint four border edges in the brand blue color.
                let border_brush = CreateSolidBrush(BORDER_COLOR);
                let top = RECT {
                    left: 0,
                    top: 0,
                    right: w,
                    bottom: BORDER_PX,
                };
                let bottom = RECT {
                    left: 0,
                    top: h - BORDER_PX,
                    right: w,
                    bottom: h,
                };
                let left = RECT {
                    left: 0,
                    top: 0,
                    right: BORDER_PX,
                    bottom: h,
                };
                let right = RECT {
                    left: w - BORDER_PX,
                    top: 0,
                    right: w,
                    bottom: h,
                };
                FillRect(hdc, &top, border_brush);
                FillRect(hdc, &bottom, border_brush);
                FillRect(hdc, &left, border_brush);
                FillRect(hdc, &right, border_brush);
                let _ = DeleteObject(border_brush.into());

                let _ = EndPaint(hwnd, &ps);
                LRESULT(0)
            }
            WM_APP_MOVE_BORDER => {
                let (x, y) = decode_i32_pair(wparam.0);
                let (w, h) = decode_i32_pair(lparam.0 as usize);
                let _ = SetWindowPos(
                    hwnd,
                    Some(HWND_TOPMOST),
                    x,
                    y,
                    w,
                    h,
                    SWP_NOACTIVATE | SWP_SHOWWINDOW,
                );
                // Force a repaint of the new bounds.
                let _ = InvalidateRect(Some(hwnd), None, true);
                LRESULT(0)
            }
            WM_APP_SET_BORDER_VISIBLE => {
                let cmd = if wparam.0 != 0 {
                    SW_SHOWNOACTIVATE
                } else {
                    SW_HIDE
                };
                let _ = ShowWindow(hwnd, cmd);
                LRESULT(0)
            }
            WM_CLOSE => {
                let _ = DestroyWindow(hwnd);
                LRESULT(0)
            }
            WM_DESTROY => {
                BORDER_HWND.store(0, Ordering::SeqCst);
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

fn spawn_border_tracker(session: u64, target_hwnd_raw: isize, border_hwnd_raw: isize) {
    std::thread::spawn(move || {
        let target_hwnd = HWND(target_hwnd_raw as *mut _);
        let border_hwnd = HWND(border_hwnd_raw as *mut _);
        let mut last_rect: Option<RECT> = None;
        let mut last_visible: Option<bool> = None;

        loop {
            if BORDER_SESSION.load(Ordering::SeqCst) != session
                || BORDER_HWND.load(Ordering::SeqCst) != border_hwnd_raw
            {
                break;
            }

            let is_valid = unsafe { IsWindow(Some(target_hwnd)).as_bool() };
            if !is_valid {
                post_border_visibility(border_hwnd, false);
                break;
            }

            let is_visible = unsafe {
                IsWindowVisible(target_hwnd).as_bool() && !IsIconic(target_hwnd).as_bool()
            };
            if last_visible != Some(is_visible) {
                post_border_visibility(border_hwnd, is_visible);
                last_visible = Some(is_visible);
            }

            if is_visible
                && let Some(rect) = get_window_bounds(target_hwnd)
                && last_rect != Some(rect)
            {
                post_border_bounds(border_hwnd, rect);
                last_rect = Some(rect);
            }

            std::thread::sleep(std::time::Duration::from_millis(BORDER_TRACK_POLL_MS));
        }
    });
}

/// Show a blue recording border that continuously tracks the target window.
/// Spawns a background thread that owns the overlay window and its message loop.
pub fn show_capture_border(target_hwnd: HWND) {
    // Close any leftover border first.
    hide_capture_border();

    let Some(initial_rect) = get_window_bounds(target_hwnd) else {
        return;
    };
    let session = BORDER_SESSION.fetch_add(1, Ordering::SeqCst) + 1;
    let target_hwnd_raw = target_hwnd.0 as isize;
    let x = initial_rect.left;
    let y = initial_rect.top;
    let w = initial_rect.right - initial_rect.left;
    let h = initial_rect.bottom - initial_rect.top;

    std::thread::spawn(move || unsafe {
        let hinstance = match GetModuleHandleW(None) {
            Ok(h) => h,
            Err(_) => return,
        };

        REGISTER_BORDER_CLASS.call_once(|| {
            let _ = RegisterClassExW(&WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(border_wnd_proc),
                hInstance: hinstance.into(),
                lpszClassName: windows::core::w!("SGTCaptureBorderClass"),
                hbrBackground: HBRUSH(std::ptr::null_mut()),
                ..Default::default()
            });
        });

        let hwnd = match CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            windows::core::w!("SGTCaptureBorderClass"),
            windows::core::w!(""),
            WS_POPUP | WS_VISIBLE,
            x,
            y,
            w,
            h,
            None,
            None,
            Some(hinstance.into()),
            None,
        ) {
            Ok(h) => h,
            Err(e) => {
                eprintln!("[CaptureBorder] CreateWindowExW failed: {e}");
                return;
            }
        };

        // Make KEY_COLOR pixels fully transparent; everything else is opaque.
        let _ = SetLayeredWindowAttributes(hwnd, KEY_COLOR, 0, LWA_COLORKEY);

        BORDER_HWND.store(hwnd.0 as isize, Ordering::SeqCst);
        spawn_border_tracker(session, target_hwnd_raw, hwnd.0 as isize);

        let mut msg = MSG::default();
        loop {
            match GetMessageW(&mut msg, None, 0, 0).0 {
                -1 | 0 => break,
                _ => {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        }
    });
}

/// Remove the capture border overlay (no-op if not shown).
pub fn hide_capture_border() {
    BORDER_SESSION.fetch_add(1, Ordering::SeqCst);
    let val = BORDER_HWND.load(Ordering::SeqCst);
    if val != 0 {
        unsafe {
            let _ = PostMessageW(Some(HWND(val as *mut _)), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }
}
