use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::{
    BeginPaint, EndPaint, GetMonitorInfoW, MONITOR_DEFAULTTONULL, MONITOR_DEFAULTTOPRIMARY,
    MONITORINFO, MonitorFromPoint, PAINTSTRUCT,
};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::overlay::result::button_canvas;
use crate::overlay::result::state::WINDOW_STATES;

pub const WM_UNDO_CLICK: u32 = WM_USER + 210;
pub const WM_REDO_CLICK: u32 = WM_USER + 211;
pub const WM_COPY_CLICK: u32 = WM_USER + 212;
pub const WM_EDIT_CLICK: u32 = WM_USER + 213;
pub const WM_BACK_CLICK: u32 = WM_USER + 214;
pub const WM_FORWARD_CLICK: u32 = WM_USER + 215;
pub const WM_SPEAKER_CLICK: u32 = WM_USER + 216;
pub const WM_DOWNLOAD_CLICK: u32 = WM_USER + 217;
pub const WM_CLOSE_GROUP_CLICK: u32 = WM_USER + 219;

pub unsafe fn handle_destroy(hwnd: HWND) -> LRESULT {
    unsafe {
        super::super::scene_compositor::remove_window(hwnd);
        if let Some(state) = WINDOW_STATES.lock().unwrap().remove(&(hwnd.0 as isize)) {
            if let Some(token) = state.cancellation_token {
                token.cancel();
            }
            if state.tts_request_id != 0 {
                crate::api::tts::TTS_MANAGER.stop_if_active(state.tts_request_id);
            }
        }
        let _ = KillTimer(Some(hwnd), 3);
        button_canvas::unregister_markdown_window(hwnd);
        LRESULT(0)
    }
}

pub unsafe fn handle_paint(hwnd: HWND) -> LRESULT {
    unsafe {
        let mut paint = PAINTSTRUCT::default();
        let _ = BeginPaint(hwnd, &mut paint);
        let _ = EndPaint(hwnd, &paint);
        LRESULT(0)
    }
}

pub unsafe fn handle_display_change(hwnd: HWND) -> LRESULT {
    unsafe {
        let mut rect = RECT::default();
        if GetWindowRect(hwnd, &mut rect).is_ok() {
            let center = POINT {
                x: (rect.left + rect.right) / 2,
                y: (rect.top + rect.bottom) / 2,
            };
            if MonitorFromPoint(center, MONITOR_DEFAULTTONULL).is_invalid() {
                let monitor = MonitorFromPoint(POINT::default(), MONITOR_DEFAULTTOPRIMARY);
                let mut info = MONITORINFO {
                    cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                    ..Default::default()
                };
                if GetMonitorInfoW(monitor, &mut info).as_bool() {
                    let width = rect.right - rect.left;
                    let height = rect.bottom - rect.top;
                    let x = info.rcWork.left + (info.rcWork.right - info.rcWork.left - width) / 2;
                    let y = info.rcWork.top + (info.rcWork.bottom - info.rcWork.top - height) / 2;
                    let _ = SetWindowPos(
                        hwnd,
                        None,
                        x,
                        y,
                        0,
                        0,
                        SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                    );
                }
            }
        }
        LRESULT(0)
    }
}

pub unsafe fn handle_show_window(hwnd: HWND, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        let showing = wparam.0 != 0;
        super::super::scene_compositor::sync_window(hwnd, showing);
        if showing {
            SetTimer(Some(hwnd), 3, 16, None);
            button_canvas::register_markdown_window(hwnd);
        }
        DefWindowProcW(hwnd, WM_SHOWWINDOW, wparam, lparam)
    }
}

pub unsafe fn handle_back_click(hwnd: HWND) -> LRESULT {
    super::super::scene_compositor::go_back(hwnd);
    LRESULT(0)
}

pub unsafe fn handle_forward_click(hwnd: HWND) -> LRESULT {
    super::super::scene_compositor::go_forward(hwnd);
    LRESULT(0)
}

pub unsafe fn handle_download_click(hwnd: HWND) -> LRESULT {
    let text = WINDOW_STATES
        .lock()
        .unwrap()
        .get(&(hwnd.0 as isize))
        .map(|state| state.full_text.clone())
        .unwrap_or_default();
    if !text.is_empty() {
        super::super::markdown_view::save_html_file(&text);
    }
    LRESULT(0)
}

pub unsafe fn handle_close_group_click(hwnd: HWND) -> LRESULT {
    crate::overlay::result::trigger_close_group(hwnd);
    LRESULT(0)
}
