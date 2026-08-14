use super::super::html::{get_localized_badge_text, get_localized_image_badge_text};
use super::super::state::*;
use crate::APP;
use std::sync::atomic::Ordering;
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

pub(super) unsafe extern "system" fn tag_wnd_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match message {
            WM_APP_SHOW => {
                let generation = wparam.0 as u64;
                if !SELECTION_LIFECYCLE.is_current(generation)
                    || !TEXT_BADGE_VISIBLE.load(Ordering::SeqCst)
                {
                    return LRESULT(0);
                }
                TEXT_BADGE_VISIBLE.store(true, Ordering::SeqCst);
                let _ = KillTimer(Some(hwnd), 1);
                let lang = current_language();
                let text =
                    get_localized_badge_text(&lang, crate::overlay::continuous_mode::is_active());
                crate::overlay::status_compositor::selection_show(cursor_rect(), text);
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                LRESULT(0)
            }
            WM_APP_HIDE => {
                let generation = wparam.0 as u64;
                if !SELECTION_LIFECYCLE.is_current(generation) {
                    return LRESULT(0);
                }
                TEXT_BADGE_VISIBLE.store(false, Ordering::SeqCst);
                crate::overlay::status_compositor::selection_hide();
                let _ = SetTimer(Some(hwnd), 1, 150, None);
                LRESULT(0)
            }
            WM_APP_SHOW_IMAGE_BADGE => {
                let _ = KillTimer(Some(hwnd), 2);
                let text = get_localized_image_badge_text(&current_language());
                crate::overlay::status_compositor::image_badge_show(cursor_rect(), text);
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                LRESULT(0)
            }
            WM_APP_HIDE_IMAGE_BADGE => {
                crate::overlay::status_compositor::image_badge_hide();
                let _ = SetTimer(Some(hwnd), 2, 150, None);
                LRESULT(0)
            }
            WM_APP_UPDATE_CONTINUOUS => {
                if TEXT_BADGE_VISIBLE.load(Ordering::SeqCst) {
                    let text = get_localized_badge_text(&current_language(), true);
                    crate::overlay::status_compositor::selection_update(false, text);
                }
                LRESULT(0)
            }
            WM_APP_RESTORE_AFTER_CAPTURE => {
                let _ = crate::overlay::status_compositor::set_selection_capture_visible(true);
                LRESULT(0)
            }
            WM_TIMER if wparam.0 == 1 => {
                let _ = KillTimer(Some(hwnd), 1);
                let initial = INITIAL_TEXT_GLOBAL.lock().unwrap().clone();
                reset_ui_state(&initial);
                hide_controller_if_idle(hwnd);
                LRESULT(0)
            }
            WM_TIMER if wparam.0 == 2 => {
                let _ = KillTimer(Some(hwnd), 2);
                hide_controller_if_idle(hwnd);
                LRESULT(0)
            }
            WM_CLOSE => {
                let _ = KillTimer(Some(hwnd), 1);
                let _ = KillTimer(Some(hwnd), 2);
                crate::overlay::status_compositor::selection_hide();
                crate::overlay::status_compositor::image_badge_hide();
                let _ = ShowWindow(hwnd, SW_HIDE);
                LRESULT(0)
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        }));
        result.unwrap_or_else(|_| DefWindowProcW(hwnd, message, wparam, lparam))
    }
}

fn current_language() -> String {
    APP.try_lock()
        .map(|app| app.config.ui_language.clone())
        .unwrap_or_else(|_| "en".to_string())
}

fn cursor_rect() -> crate::overlay::status_compositor::protocol::PhysicalRect {
    let mut point = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut point);
    }
    crate::overlay::status_compositor::physical_rect(
        point.x + OFFSET_X,
        point.y + OFFSET_Y,
        BADGE_WIDTH,
        BADGE_HEIGHT,
    )
}

fn hide_controller_if_idle(hwnd: HWND) {
    if !TEXT_BADGE_VISIBLE.load(Ordering::SeqCst)
        && !IMAGE_CONTINUOUS_BADGE_VISIBLE.load(Ordering::SeqCst)
    {
        unsafe {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
    }
}
