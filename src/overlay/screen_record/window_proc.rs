// Window procedure handler for the screen record window.

use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::*;
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::WindowsAndMessaging::*;
use wry::Rect;

use super::{
    push_settings_to_webview, SR_WEBVIEW, WM_APP_RUN_SCRIPT, WM_APP_SHOW, WM_APP_TOGGLE,
    WM_APP_UPDATE_SETTINGS,
};

pub(super) unsafe extern "system" fn sr_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        match msg {
            WM_APP_SHOW => {
                let _ = ShowWindow(hwnd, SW_SHOW);
                let _ = SetForegroundWindow(hwnd);
                let _ = SetFocus(Some(hwnd));
                // Push current theme/lang on show
                push_settings_to_webview();
                LRESULT(0)
            }
            WM_APP_UPDATE_SETTINGS => {
                push_settings_to_webview();
                LRESULT(0)
            }
            WM_ERASEBKGND => {
                LRESULT(1) // Suppress -- WebView covers full client area
            }
            WM_CLOSE => {
                let _ = ShowWindow(hwnd, SW_HIDE);
                LRESULT(0)
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            WM_NCCALCSIZE => {
                if wparam.0 == 1 {
                    let params = &mut *(lparam.0 as *mut NCCALCSIZE_PARAMS);
                    if IsZoomed(hwnd).as_bool() {
                        let frame_x =
                            GetSystemMetrics(SM_CXFRAME) + GetSystemMetrics(SM_CXPADDEDBORDER);
                        let frame_y =
                            GetSystemMetrics(SM_CYFRAME) + GetSystemMetrics(SM_CXPADDEDBORDER);
                        params.rgrc[0].left += frame_x;
                        params.rgrc[0].top += frame_y;
                        params.rgrc[0].right -= frame_x;
                        params.rgrc[0].bottom -= frame_y;
                    }
                }
                LRESULT(0)
            }
            WM_NCHITTEST => {
                handle_nchittest(hwnd, lparam)
            }
            WM_GETMINMAXINFO => {
                let info = &mut *(lparam.0 as *mut MINMAXINFO);
                info.ptMinTrackSize.x = 800;
                info.ptMinTrackSize.y = 500;
                LRESULT(0)
            }
            WM_EXITSIZEMOVE => {
                // Persist restored (non-maximized/minimized) screen-record window size.
                if !IsZoomed(hwnd).as_bool() && !IsIconic(hwnd).as_bool() {
                    let mut rect = RECT::default();
                    let _ = GetWindowRect(hwnd, &mut rect);
                    let w = (rect.right - rect.left).max(800);
                    let h = (rect.bottom - rect.top).max(500);
                    {
                        let mut app = crate::APP.lock().unwrap();
                        app.config.screen_record_window_size = (w, h);
                        crate::config::save_config(&app.config);
                    }
                }
                LRESULT(0)
            }
            WM_SIZE => {
                SR_WEBVIEW.with(|wv| {
                    if let Some(webview) = wv.borrow().as_ref() {
                        let mut r = RECT::default();
                        let _ = GetClientRect(hwnd, &mut r);
                        let w = (r.right - r.left).max(0);
                        let h = (r.bottom - r.top).max(0);
                        let _ = webview.set_bounds(Rect {
                            position: wry::dpi::Position::Physical(
                                wry::dpi::PhysicalPosition::new(0, 0),
                            ),
                            size: wry::dpi::Size::Physical(wry::dpi::PhysicalSize::new(
                                w as u32, h as u32,
                            )),
                        });
                    }
                });
                LRESULT(0)
            }
            WM_APP_TOGGLE => {
                SR_WEBVIEW.with(|wv| {
                    if let Some(webview) = wv.borrow().as_ref() {
                        let _ = webview.evaluate_script(
                            "window.dispatchEvent(new CustomEvent('toggle-recording'));",
                        );
                    }
                });
                LRESULT(0)
            }
            WM_SETFOCUS => {
                SR_WEBVIEW.with(|wv| {
                    if let Some(webview) = wv.borrow().as_ref() {
                        let _ = webview.focus();
                    }
                });
                LRESULT(0)
            }
            WM_APP_RUN_SCRIPT => {
                let script_ptr = lparam.0 as *mut String;
                if !script_ptr.is_null() {
                    let script = Box::from_raw(script_ptr);
                    SR_WEBVIEW.with(|wv| {
                        if let Some(webview) = wv.borrow().as_ref() {
                            let _ = webview.evaluate_script(&script);
                        }
                    });
                }
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

unsafe fn handle_nchittest(hwnd: HWND, lparam: LPARAM) -> LRESULT {
    unsafe {
        let x = lparam.0 as i16 as i32;
        let y = (lparam.0 >> 16) as i16 as i32;

        // GetWindowRect includes the invisible DWM shadow (~7px each side).
        // Use DWMWA_EXTENDED_FRAME_BOUNDS for the actual visible rect so resize
        // zones are measured from the visible edge, not from inside the shadow.
        let mut rect = RECT::default();
        let _ = GetWindowRect(hwnd, &mut rect);
        let mut frame = rect;
        let _ = DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut frame as *mut _ as *mut std::ffi::c_void,
            std::mem::size_of::<RECT>() as u32,
        );

        let border = 6; // px into visible area; shadow zone is always resize
        let title_height = 44;

        // Resize zones: shadow (outside visible) + `border` px inside visible
        let top = frame.top + border;
        let bottom = frame.bottom - border;
        let left = frame.left + border;
        let right = frame.right - border;

        if y < top {
            if x < left {
                return LRESULT(HTTOPLEFT as isize);
            }
            if x > right {
                return LRESULT(HTTOPRIGHT as isize);
            }
            return LRESULT(HTTOP as isize);
        }
        if y > bottom {
            if x < left {
                return LRESULT(HTBOTTOMLEFT as isize);
            }
            if x > right {
                return LRESULT(HTBOTTOMRIGHT as isize);
            }
            return LRESULT(HTBOTTOM as isize);
        }
        if x < left {
            return LRESULT(HTLEFT as isize);
        }
        if x > right {
            return LRESULT(HTRIGHT as isize);
        }

        if y < frame.top + title_height {
            return LRESULT(HTCLIENT as isize);
        }

        LRESULT(HTCLIENT as isize)
    }
}
