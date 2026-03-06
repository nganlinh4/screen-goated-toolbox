// --- CAPTURE BORDER OVERLAY ---
// Draws a colored border around the target window during window capture recording.
// Uses a WS_EX_LAYERED window with LWA_COLORKEY so only the border is visible.
// Display capture uses the system yellow border (DrawBorderSettings::Default).
// Window capture uses this custom blue overlay for a distinct visual indicator.

use std::sync::Once;
use std::sync::atomic::{AtomicIsize, Ordering};
use windows::Win32::Foundation::*;
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

static REGISTER_BORDER_CLASS: Once = Once::new();
static BORDER_HWND: AtomicIsize = AtomicIsize::new(0);

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
                // wparam = x<<32|y, lparam = w<<32|h packed as two i32 in isize
                let x = (wparam.0 >> 32) as i32;
                let y = (wparam.0 & 0xFFFF_FFFF) as i32;
                let w = (lparam.0 >> 32) as i32;
                let h = (lparam.0 & 0xFFFF_FFFF) as i32;
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

/// Show a blue recording border around the given screen rect.
/// Spawns a background thread that owns the window and its message loop.
pub fn show_capture_border(x: i32, y: i32, w: i32, h: i32) {
    // Close any leftover border first.
    hide_capture_border();

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
    let val = BORDER_HWND.load(Ordering::SeqCst);
    if val != 0 {
        unsafe {
            let _ = PostMessageW(Some(HWND(val as *mut _)), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }
}
