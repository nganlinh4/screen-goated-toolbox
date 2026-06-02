use super::super::html::{get_localized_badge_text, get_localized_image_badge_text};
use super::super::state::*;
use crate::APP;
use std::sync::atomic::Ordering;
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

pub(super) unsafe extern "system" fn tag_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        let lang = {
            if let Ok(app) = APP.try_lock() {
                app.config.ui_language.clone()
            } else {
                "en".to_string()
            }
        };

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match msg {
            WM_APP_SHOW => {
                crate::log_info!("[Badge] WM_APP_SHOW received");
                TEXT_BADGE_VISIBLE.store(true, Ordering::SeqCst);
                let _ = KillTimer(Some(hwnd), 1);

                let mut pt = POINT::default();
                let _ = GetCursorPos(&mut pt);
                let _ = MoveWindow(
                    hwnd,
                    pt.x + OFFSET_X,
                    pt.y + OFFSET_Y,
                    BADGE_WIDTH,
                    BADGE_HEIGHT,
                    false,
                );

                let is_continuous = crate::overlay::continuous_mode::is_active();
                let badge_text = get_localized_badge_text(&lang, is_continuous);
                crate::log_info!(
                    "[Badge] WM_APP_SHOW: is_continuous={}, badge_text='{}'",
                    is_continuous,
                    badge_text
                );

                {
                    let state = SELECTION_STATE.lock().unwrap();
                    if let Some(wv) = state.webview.as_ref() {
                        let _ = wv.evaluate_script(&format!(
                            "updateState(false, '{}'); playEntry();",
                            badge_text
                        ));
                    }
                }
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                LRESULT(0)
            }
            WM_APP_HIDE => {
                crate::log_info!("[Badge] WM_APP_HIDE received");
                TEXT_BADGE_VISIBLE.store(false, Ordering::SeqCst);
                {
                    let state = SELECTION_STATE.lock().unwrap();
                    if let Some(wv) = state.webview.as_ref() {
                        let _ = wv.evaluate_script("playExit();");
                    }
                }
                SetTimer(Some(hwnd), 1, 150, None);
                LRESULT(0)
            }
            WM_APP_SHOW_IMAGE_BADGE => {
                crate::log_info!("[Badge] WM_APP_SHOW_IMAGE_BADGE received");
                let _ = KillTimer(Some(hwnd), 2);

                let mut pt = POINT::default();
                let _ = GetCursorPos(&mut pt);
                let _ = MoveWindow(
                    hwnd,
                    pt.x + OFFSET_X,
                    pt.y + OFFSET_Y,
                    BADGE_WIDTH,
                    BADGE_HEIGHT,
                    false,
                );

                let image_badge_text = get_localized_image_badge_text(&lang);

                {
                    let state = SELECTION_STATE.lock().unwrap();
                    if let Some(wv) = state.webview.as_ref() {
                        if !TEXT_BADGE_VISIBLE.load(Ordering::SeqCst) {
                            let _ = wv.evaluate_script("playExit();");
                        }
                        let _ = wv.evaluate_script(&format!(
                            "updateImageText('{}'); showImageBadge();",
                            image_badge_text
                        ));
                    }
                }
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                LRESULT(0)
            }
            WM_APP_HIDE_IMAGE_BADGE => {
                crate::log_info!("[Badge] WM_APP_HIDE_IMAGE_BADGE received");
                {
                    let state = SELECTION_STATE.lock().unwrap();
                    if let Some(wv) = state.webview.as_ref() {
                        let _ = wv.evaluate_script("hideImageBadge();");
                    }
                }
                SetTimer(Some(hwnd), 2, 150, None);
                LRESULT(0)
            }
            WM_APP_UPDATE_CONTINUOUS => {
                crate::log_info!("[Badge] WM_APP_UPDATE_CONTINUOUS received");
                if TEXT_BADGE_VISIBLE.load(Ordering::SeqCst) {
                    let continuous_text = get_localized_badge_text(&lang, true);
                    crate::log_info!("[Badge] Updating text to: '{}'", continuous_text);
                    {
                        let state = SELECTION_STATE.lock().unwrap();
                        if let Some(wv) = state.webview.as_ref() {
                            let _ = wv.evaluate_script(&format!(
                                "updateState(false, '{}')",
                                continuous_text
                            ));
                        }
                    }
                }
                LRESULT(0)
            }
            WM_APP_RESTORE_AFTER_CAPTURE => {
                crate::log_info!("[Badge] WM_APP_RESTORE_AFTER_CAPTURE received");
                let text_visible = TEXT_BADGE_VISIBLE.load(Ordering::SeqCst);
                let image_visible = IMAGE_CONTINUOUS_BADGE_VISIBLE.load(Ordering::SeqCst);
                if text_visible || image_visible {
                    let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                }
                LRESULT(0)
            }
            WM_TIMER => {
                if wparam.0 == 1 {
                    let _ = KillTimer(Some(hwnd), 1);
                    {
                        let initial_text = INITIAL_TEXT_GLOBAL.lock().unwrap();
                        reset_ui_state(&initial_text);
                    }
                    if !IMAGE_CONTINUOUS_BADGE_VISIBLE.load(Ordering::SeqCst)
                        && !TEXT_BADGE_VISIBLE.load(Ordering::SeqCst)
                    {
                        let _ = ShowWindow(hwnd, SW_HIDE);
                    }
                } else if wparam.0 == 2 {
                    let _ = KillTimer(Some(hwnd), 2);
                    if !TEXT_BADGE_VISIBLE.load(Ordering::SeqCst)
                        && !IMAGE_CONTINUOUS_BADGE_VISIBLE.load(Ordering::SeqCst)
                    {
                        let _ = ShowWindow(hwnd, SW_HIDE);
                    }
                }
                LRESULT(0)
            }
            WM_CLOSE => {
                let _ = KillTimer(Some(hwnd), 1);
                let initial_text = INITIAL_TEXT_GLOBAL.lock().unwrap();
                reset_ui_state(&initial_text);
                let _ = ShowWindow(hwnd, SW_HIDE);
                LRESULT(0)
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }));
        match result {
            Ok(lresult) => lresult,
            Err(_) => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}
