// Window procedure handler for the screen record window.

use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::*;
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::WindowsAndMessaging::*;
use wry::Rect;

use super::{
    SR_WEBVIEW, WM_APP_RUN_SCRIPT, WM_APP_SHOW, WM_APP_TOGGLE, WM_APP_UPDATE_SETTINGS,
    cleanup_on_app_exit, push_settings_to_webview,
};

const WM_APP_SAFE_CLOSE: u32 = WM_USER + 114;
thread_local! {
    static CLOSE_ATTEMPT: std::cell::Cell<Option<super::close_show::CloseAttempt>> = const { std::cell::Cell::new(None) };
}

pub(super) fn take_close_attempt() -> Option<super::close_show::CloseAttempt> {
    CLOSE_ATTEMPT.with(|attempt| attempt.take())
}

pub(super) unsafe extern "system" fn sr_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        match msg {
            WM_APP_SHOW => {
                if super::close_show::defer_show_if_closing() {
                    return LRESULT(0);
                }
                super::shutdown::reopen_recording_admission();
                let _ = ShowWindow(hwnd, SW_SHOW);
                let _ = SetForegroundWindow(hwnd);
                let _ = SetFocus(Some(hwnd));
                super::startup_trace::log("window-shown");
                // The export compositor pipeline takes 8-10s to compile on first
                // use. Build it now so it is ready long before the user exports.
                super::gpu_export::prewarm_shared_gpu_context();
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
                let Some(attempt) = super::close_show::begin_close(hwnd.0 as isize) else {
                    if wparam.0 != 0 {
                        let _ = super::post_script(format!(
                            "window.dispatchEvent(new CustomEvent('sr-shutdown-failed',{{detail:{{subtitleOverrideId:{}}}}}));",
                            wparam.0
                        ));
                    }
                    return LRESULT(0);
                };
                CLOSE_ATTEMPT.with(|current| current.set(Some(attempt)));
                let override_id = wparam.0;
                let spawned = std::thread::Builder::new()
                    .name("recorder-close".into())
                    .spawn(move || match super::shutdown::prepare(override_id) {
                        Ok(owner) => {
                            if !super::close_show::mark_prepared(attempt, owner) {
                                super::shutdown::cancel_prepared_owner(owner);
                                return;
                            }
                            if PostMessageW(
                                Some(HWND(attempt.hwnd as *mut _)),
                                WM_APP_SAFE_CLOSE,
                                WPARAM(attempt.id),
                                LPARAM(0),
                            )
                            .is_err()
                            {
                                super::shutdown::cancel_prepared_owner(owner);
                                if matches!(super::close_show::finish_close(attempt), Some((true, _))) { super::show_screen_record(); }
                            }
                        }
                        Err(error) => {
                            crate::log_info!("[ScreenRecord] close canceled: {error:#}");
                            let show = matches!(super::close_show::finish_close(attempt), Some((true, _)));
                            let _ = super::post_script(format!(
                                "window.dispatchEvent(new CustomEvent('sr-shutdown-failed',{{detail:{{subtitleOverrideId:{override_id}}}}}));"
                            ));
                            if show { super::show_screen_record(); }
                        }
                    });
                if spawned.is_err() {
                    let show = matches!(super::close_show::finish_close(attempt), Some((true, _)));
                    let _ = super::post_script(format!(
                        "window.dispatchEvent(new CustomEvent('sr-shutdown-failed',{{detail:{{subtitleOverrideId:{override_id}}}}}));"
                    ));
                    if show {
                        super::show_screen_record();
                    }
                }
                LRESULT(0)
            }
            WM_APP_SAFE_CLOSE => {
                let current = CLOSE_ATTEMPT.with(std::cell::Cell::get);
                if let Some(attempt) = current
                    .filter(|attempt| attempt.hwnd == hwnd.0 as isize && attempt.id == wparam.0)
                    && let Some(owner) = super::close_show::prepared_owner(attempt)
                {
                    if super::shutdown::commit_prepared_owner(owner) {
                        cleanup_on_app_exit();
                        if DestroyWindow(hwnd).is_err() {
                            super::shutdown::release_closed_window_owner(owner);
                            if IsWindow(Some(hwnd)).as_bool() {
                                super::shutdown::reopen_recording_admission();
                            }
                            let show =
                                matches!(super::close_show::finish_close(attempt), Some((true, _)));
                            let _ = super::post_script(
                                "window.dispatchEvent(new Event('sr-shutdown-failed'));"
                                    .to_string(),
                            );
                            if show {
                                super::show_screen_record();
                            }
                        }
                    } else {
                        super::shutdown::release_closed_window_owner(owner);
                        let show =
                            matches!(super::close_show::finish_close(attempt), Some((true, _)));
                        let _ = super::post_script(
                            "window.dispatchEvent(new Event('sr-shutdown-failed'));".to_string(),
                        );
                        if show {
                            super::show_screen_record();
                        }
                    }
                }
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
            WM_NCHITTEST => handle_nchittest(hwnd, lparam),
            WM_GETMINMAXINFO => {
                let info = &mut *(lparam.0 as *mut MINMAXINFO);
                let area = crate::overlay::display_placement::window_work_area(hwnd);
                info.ptMinTrackSize.x = 800.min(area.width);
                info.ptMinTrackSize.y = 500.min(area.height);
                LRESULT(0)
            }
            WM_EXITSIZEMOVE => {
                // Persist restored (non-maximized/minimized) screen-record window size.
                if !IsZoomed(hwnd).as_bool() && !IsIconic(hwnd).as_bool() {
                    let mut rect = RECT::default();
                    let _ = GetWindowRect(hwnd, &mut rect);
                    let w = (rect.right - rect.left).max(800);
                    let h = (rect.bottom - rect.top).max(500);
                    if let Err(error) = super::worker_settings::notify(
                        sgt_recorder_protocol::SettingsChange::WindowSize((w, h)),
                    ) {
                        crate::log_info!("[ScreenRecord] could not persist window size: {error:#}");
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
