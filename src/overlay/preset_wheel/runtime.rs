use super::html::get_wheel_template;
use super::state::{
    HwndWrapper, IS_WARMED_UP, IS_WARMING_UP, OVERLAY_HWND, PENDING_CSS, PENDING_DISMISS_LABEL,
    PENDING_ITEMS_HTML, PENDING_POS, PENDING_SIZE, REGISTER_OVERLAY_CLASS, REGISTER_WHEEL_CLASS,
    WHEEL_ACTIVE, WHEEL_HEIGHT, WHEEL_HWND, WHEEL_LIVENESS, WHEEL_PAGE_READY_ID,
    WHEEL_PROTOCOL_LOCK, WHEEL_READY_ID, WHEEL_REQUEST_ID, WHEEL_RESULT, WHEEL_WEB_CONTEXT,
    WHEEL_WEBVIEW, WHEEL_WIDTH, WM_APP_HIDE, WM_APP_REAL_SHOW, WM_APP_SHOW,
};
use std::sync::atomic::Ordering;
use std::time::Instant;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DWMWA_WINDOW_CORNER_PREFERENCE, DwmExtendFrameIntoClientArea, DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Gdi::{HBRUSH, InvalidateRect};
use windows::Win32::System::Com::{CoInitialize, CoUninitialize};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    GetSystemMetrics, HWND_TOPMOST, IDC_ARROW, IsWindow, KillTimer, LWA_ALPHA, LoadCursorW, MSG,
    PostMessageW, PostQuitMessage, RegisterClassW, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN,
    SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SW_HIDE, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOMOVE,
    SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW, SetLayeredWindowAttributes, SetTimer, SetWindowPos,
    ShowWindow, TranslateMessage, WM_CLOSE, WM_DESTROY, WM_DPICHANGED, WM_ERASEBKGND, WM_KEYDOWN,
    WM_LBUTTONDOWN, WM_RBUTTONDOWN, WM_TIMER, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
};
use windows::core::w;
use wry::{Rect, WebViewBuilder};

struct WheelThreadLifecycle;

impl Drop for WheelThreadLifecycle {
    fn drop(&mut self) {
        let _protocol = WHEEL_PROTOCOL_LOCK.lock().unwrap();
        IS_WARMING_UP.store(true, Ordering::SeqCst);
        IS_WARMED_UP.store(false, Ordering::SeqCst);
        WHEEL_HWND.store(0, Ordering::SeqCst);
        OVERLAY_HWND.store(0, Ordering::SeqCst);
        WHEEL_PAGE_READY_ID.store(0, Ordering::SeqCst);
        WHEEL_READY_ID.store(0, Ordering::SeqCst);
        WHEEL_ACTIVE.store(false, Ordering::SeqCst);
        WHEEL_LIVENESS.lock().unwrap().clear();
        let _ = WHEEL_RESULT.compare_exchange(-1, -2, Ordering::SeqCst, Ordering::SeqCst);
        IS_WARMING_UP.store(false, Ordering::SeqCst);
    }
}

pub(crate) fn internal_create_window_loop() {
    let _lifecycle = WheelThreadLifecycle;
    unsafe {
        let _ = CoInitialize(None);

        let instance = GetModuleHandleW(None).unwrap_or_default();
        let screen_x = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let screen_y = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let screen_w = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let screen_h = GetSystemMetrics(SM_CYVIRTUALSCREEN);

        let overlay_class = w!("SGTWheelOverlayPersistent");
        REGISTER_OVERLAY_CLASS.call_once(|| {
            let wc = WNDCLASSW {
                lpfnWndProc: Some(overlay_wnd_proc),
                hInstance: instance.into(),
                lpszClassName: overlay_class,
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                hbrBackground: HBRUSH(std::ptr::null_mut()),
                ..Default::default()
            };
            RegisterClassW(&wc);
        });

        let overlay_hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED | WS_EX_NOACTIVATE,
            overlay_class,
            w!("WheelOverlay"),
            WS_POPUP,
            screen_x,
            screen_y,
            screen_w,
            screen_h - 1,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .unwrap_or_default();

        OVERLAY_HWND.store(overlay_hwnd.0 as isize, Ordering::SeqCst);
        let _ = SetLayeredWindowAttributes(overlay_hwnd, COLORREF(0), 1, LWA_ALPHA);

        let class_name = w!("SGTPresetWheelPersistent");
        REGISTER_WHEEL_CLASS.call_once(|| {
            let wc = WNDCLASSW {
                lpfnWndProc: Some(wheel_wnd_proc),
                hInstance: instance.into(),
                lpszClassName: class_name,
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                hbrBackground: HBRUSH(std::ptr::null_mut()),
                ..Default::default()
            };
            RegisterClassW(&wc);
        });

        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class_name,
            w!("PresetWheel"),
            WS_POPUP | WS_VISIBLE,
            -4000,
            -4000,
            2000,
            2000,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .unwrap_or_default();

        let corner_pref = 1u32;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            std::ptr::addr_of!(corner_pref) as *const _,
            std::mem::size_of_val(&corner_pref) as u32,
        );

        let margins = MARGINS {
            cxLeftWidth: -1,
            cxRightWidth: -1,
            cyTopHeight: -1,
            cyBottomHeight: -1,
        };
        let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);

        let wrapper = HwndWrapper(hwnd);

        WHEEL_WEB_CONTEXT.with(|ctx| {
            if ctx.borrow().is_none() {
                *ctx.borrow_mut() = Some(crate::overlay::webview_runtime::create_context(
                    crate::overlay::webview_runtime::Profile::Common,
                ));
            }
        });

        let webview_res = {
            let init = crate::overlay::webview_init::acquire("preset-wheel");
            let build_res = WHEEL_WEB_CONTEXT.with(|ctx| {
                let mut ctx_ref = ctx.borrow_mut();
                let builder = if let Some(web_ctx) = ctx_ref.as_mut() {
                    WebViewBuilder::new_with_web_context(web_ctx)
                } else {
                    WebViewBuilder::new()
                };

                let ui_language = crate::APP
                    .lock()
                    .map(|app| app.config.ui_language.clone())
                    .unwrap_or_else(|_| "en".to_string());
                let locale = crate::gui::locale::LocaleText::get(&ui_language);
                let template_html = get_wheel_template(true, locale.overlay.preset_wheel_cancel);
                let page_url = crate::overlay::html_components::font_manager::store_html_page(
                    template_html.clone(),
                )
                .unwrap_or_else(|| {
                    format!("data:text/html,{}", urlencoding::encode(&template_html))
                });

                builder
                    .with_transparent(true)
                    .with_background_color((0, 0, 0, 0))
                    .with_url(&page_url)
                    .with_bounds(Rect {
                        position: wry::dpi::Position::Physical(wry::dpi::PhysicalPosition::new(
                            0, 0,
                        )),
                        size: wry::dpi::Size::Physical(wry::dpi::PhysicalSize::new(
                            WHEEL_WIDTH as u32,
                            WHEEL_HEIGHT as u32,
                        )),
                    })
                    .with_ipc_handler(move |msg: wry::http::Request<String>| {
                        let Some((id, event)) = parse_wheel_message(msg.body()) else {
                            return;
                        };
                        match event {
                            WheelEvent::Ready => {
                                {
                                    let _protocol = WHEEL_PROTOCOL_LOCK.lock().unwrap();
                                    if !request_is_active(id) {
                                        return;
                                    }
                                    WHEEL_PAGE_READY_ID.store(id, Ordering::SeqCst);
                                    WHEEL_LIVENESS.lock().unwrap().refresh(id, Instant::now());
                                }
                                let wheel_hwnd = HWND(WHEEL_HWND.load(Ordering::SeqCst) as *mut _);
                                if !wheel_hwnd.is_invalid() {
                                    let _ = PostMessageW(
                                        Some(wheel_hwnd),
                                        WM_APP_REAL_SHOW,
                                        WPARAM(id as usize),
                                        LPARAM(0),
                                    );
                                }
                            }
                            WheelEvent::Heartbeat => {
                                let _protocol = WHEEL_PROTOCOL_LOCK.lock().unwrap();
                                if request_is_active(id) {
                                    WHEEL_LIVENESS.lock().unwrap().refresh(id, Instant::now());
                                }
                            }
                            WheelEvent::Dismiss => hide_wheel_with_result(id, -2),
                            WheelEvent::Select(index) => hide_wheel_with_result(id, index),
                        }
                    })
                    .build(&wrapper)
            });
            if let Err(error) = &build_res {
                crate::log_info!("[PresetWheel] WebView initialization failed: {error:?}");
            }
            init.finish(build_res.is_ok());
            build_res
        };

        if let Ok(wv) = webview_res {
            crate::overlay::webview_diagnostics::attach_webview2_close_on_failure(
                "preset-wheel",
                hwnd,
                &wv,
            );
            WHEEL_WEBVIEW.with(|cell| {
                *cell.borrow_mut() = Some(wv);
            });
            let _ = ShowWindow(hwnd, SW_HIDE);
            WHEEL_HWND.store(hwnd.0 as isize, Ordering::SeqCst);
            IS_WARMED_UP.store(true, Ordering::SeqCst);
            IS_WARMING_UP.store(false, Ordering::SeqCst);
        } else {
            let _ = DestroyWindow(hwnd);
            let _ = DestroyWindow(overlay_hwnd);
            WHEEL_WEB_CONTEXT.with(|cell| {
                *cell.borrow_mut() = None;
            });
            CoUninitialize();
            return;
        }

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        IS_WARMING_UP.store(true, Ordering::SeqCst);
        IS_WARMED_UP.store(false, Ordering::SeqCst);
        WHEEL_WEBVIEW.with(|cell| {
            *cell.borrow_mut() = None;
        });
        WHEEL_WEB_CONTEXT.with(|cell| {
            *cell.borrow_mut() = None;
        });
        if IsWindow(Some(hwnd)).as_bool() {
            let _ = DestroyWindow(hwnd);
        }
        if IsWindow(Some(overlay_hwnd)).as_bool() {
            let _ = DestroyWindow(overlay_hwnd);
        }
        CoUninitialize();
    }
}

fn request_is_active(id: u64) -> bool {
    request_matches(
        id,
        WHEEL_REQUEST_ID.load(Ordering::SeqCst),
        WHEEL_ACTIVE.load(Ordering::SeqCst),
        WHEEL_RESULT.load(Ordering::SeqCst),
    )
}

fn request_matches(id: u64, current: u64, active: bool, result: i32) -> bool {
    active && result == -1 && current == id
}

#[derive(Debug, PartialEq, Eq)]
enum WheelEvent {
    Ready,
    Heartbeat,
    Dismiss,
    Select(i32),
}

fn parse_wheel_message(body: &str) -> Option<(u64, WheelEvent)> {
    if let Some(id) = body.strip_prefix("heartbeat:") {
        return Some((id.parse().ok()?, WheelEvent::Heartbeat));
    }
    if let Some(id) = body.strip_prefix("ready_to_show:") {
        return Some((id.parse().ok()?, WheelEvent::Ready));
    }
    if let Some(id) = body.strip_prefix("dismiss:") {
        return Some((id.parse().ok()?, WheelEvent::Dismiss));
    }
    let (id, index) = body.strip_prefix("select:")?.split_once(':')?;
    let index: i32 = index.parse().ok()?;
    (index >= 0).then_some((id.parse().ok()?, WheelEvent::Select(index)))
}

unsafe fn hide_wheel_with_result(id: u64, result: i32) {
    {
        let _protocol = WHEEL_PROTOCOL_LOCK.lock().unwrap();
        if !request_is_active(id)
            || WHEEL_RESULT
                .compare_exchange(-1, result, Ordering::SeqCst, Ordering::SeqCst)
                .is_err()
        {
            return;
        }
    }
    let hwnd_val = WHEEL_HWND.load(Ordering::SeqCst);
    let wheel_hwnd = HWND(hwnd_val as *mut _);
    if !wheel_hwnd.is_invalid() {
        unsafe {
            let _ = PostMessageW(
                Some(wheel_hwnd),
                WM_APP_HIDE,
                WPARAM(id as usize),
                LPARAM(0),
            );
        }
    }
}

unsafe extern "system" fn overlay_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        match msg {
            WM_LBUTTONDOWN | WM_RBUTTONDOWN => {
                hide_wheel_with_result(WHEEL_REQUEST_ID.load(Ordering::SeqCst), -2);
                LRESULT(0)
            }
            WM_CLOSE => LRESULT(0),
            WM_ERASEBKGND => LRESULT(1),
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

unsafe extern "system" fn wheel_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        match msg {
            WM_APP_SHOW => {
                if !request_is_active(wparam.0 as u64) {
                    return LRESULT(0);
                }
                let items_html = PENDING_ITEMS_HTML.lock().unwrap().clone();
                let (wheel_width, wheel_height) = *PENDING_SIZE.lock().unwrap();
                let dismiss_label = PENDING_DISMISS_LABEL.lock().unwrap().clone();
                let themed_css = PENDING_CSS.lock().unwrap().clone();

                let _ = SetWindowPos(
                    hwnd,
                    Some(HWND_TOPMOST),
                    -4000,
                    -4000,
                    0,
                    0,
                    SWP_NOACTIVATE | SWP_NOSIZE | SWP_SHOWWINDOW,
                );

                let margins = MARGINS {
                    cxLeftWidth: -1,
                    cxRightWidth: -1,
                    cyTopHeight: -1,
                    cyBottomHeight: -1,
                };
                let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);

                WHEEL_WEBVIEW.with(|wv| {
                    if let Some(webview) = wv.borrow().as_ref() {
                        let css_script = format!(
                            "document.getElementById('theme-style').textContent = `{}`;",
                            themed_css
                                .replace("\\", "\\\\")
                                .replace("`", "\\`")
                                .replace("$", "\\$")
                        );
                        let _ = webview.evaluate_script(&css_script);

                        let script = format!(
                            "window.updateContent(`{}`, `{}`, '{}');",
                            items_html
                                .replace("\\", "\\\\")
                                .replace("`", "\\`")
                                .replace("$", "\\$"),
                            dismiss_label.replace("`", "\\`").replace("$", "\\$"),
                            wparam.0
                        );
                        let _ = webview.evaluate_script(&script);
                        let _ = webview.set_bounds(Rect {
                            position: wry::dpi::Position::Physical(
                                wry::dpi::PhysicalPosition::new(0, 0),
                            ),
                            size: wry::dpi::Size::Physical(wry::dpi::PhysicalSize::new(
                                wheel_width as u32,
                                wheel_height as u32,
                            )),
                        });
                    }
                });

                SetTimer(Some(hwnd), 99, 150, None);
                LRESULT(0)
            }

            WM_APP_REAL_SHOW => {
                if !request_is_active(wparam.0 as u64) {
                    return LRESULT(0);
                }
                let _ = KillTimer(Some(hwnd), 99);
                let (target_x, target_y) = *PENDING_POS.lock().unwrap();
                let (wheel_width, wheel_height) = *PENDING_SIZE.lock().unwrap();

                let overlay_val = OVERLAY_HWND.load(Ordering::SeqCst);
                let overlay = HWND(overlay_val as *mut _);
                if !overlay.is_invalid() {
                    let _ = ShowWindow(overlay, SW_SHOWNOACTIVATE);
                    let screen_x = GetSystemMetrics(SM_XVIRTUALSCREEN);
                    let screen_y = GetSystemMetrics(SM_YVIRTUALSCREEN);
                    let screen_w = GetSystemMetrics(SM_CXVIRTUALSCREEN);
                    let screen_h = GetSystemMetrics(SM_CYVIRTUALSCREEN);
                    let _ = SetWindowPos(
                        overlay,
                        Some(HWND_TOPMOST),
                        screen_x,
                        screen_y,
                        screen_w,
                        screen_h - 1,
                        SWP_NOACTIVATE | SWP_NOMOVE,
                    );
                }

                let _ = InvalidateRect(Some(hwnd), None, true);
                let shown = SetWindowPos(
                    hwnd,
                    Some(HWND_TOPMOST),
                    target_x,
                    target_y,
                    wheel_width,
                    wheel_height,
                    SWP_NOACTIVATE,
                );
                if shown.is_ok() && WHEEL_PAGE_READY_ID.load(Ordering::SeqCst) == wparam.0 as u64 {
                    WHEEL_READY_ID.store(wparam.0 as u64, Ordering::SeqCst);
                }

                LRESULT(0)
            }

            WM_TIMER => {
                if wparam.0 == 99 {
                    let id = WHEEL_REQUEST_ID.load(Ordering::SeqCst);
                    let message = if WHEEL_PAGE_READY_ID.load(Ordering::SeqCst) == id {
                        WM_APP_REAL_SHOW
                    } else {
                        WM_APP_SHOW
                    };
                    let _ = PostMessageW(Some(hwnd), message, WPARAM(id as usize), LPARAM(0));
                }
                LRESULT(0)
            }

            WM_APP_HIDE => {
                {
                    let _protocol = WHEEL_PROTOCOL_LOCK.lock().unwrap();
                    if WHEEL_REQUEST_ID.load(Ordering::SeqCst) != wparam.0 as u64 {
                        return LRESULT(0);
                    }
                    let _ = KillTimer(Some(hwnd), 99);
                    let _ = ShowWindow(hwnd, SW_HIDE);
                    let overlay_val = OVERLAY_HWND.load(Ordering::SeqCst);
                    let overlay = HWND(overlay_val as *mut _);
                    if !overlay.is_invalid() {
                        let _ = ShowWindow(overlay, SW_HIDE);
                    }
                    WHEEL_ACTIVE.store(false, Ordering::SeqCst);
                }

                WHEEL_WEBVIEW.with(|wv| {
                    if let Some(webview) = wv.borrow().as_ref() {
                        let _ = webview
                            .evaluate_script(&format!("window.clearContent('{}');", wparam.0));
                    }
                });
                LRESULT(0)
            }

            WM_KEYDOWN => {
                if wparam.0 as u32 == 0x1B {
                    hide_wheel_with_result(WHEEL_REQUEST_ID.load(Ordering::SeqCst), -2);
                }
                LRESULT(0)
            }

            WM_DPICHANGED => {
                let rect = &*(lparam.0 as *const RECT);
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    rect.left,
                    rect.top,
                    rect.right - rect.left,
                    rect.bottom - rect.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
                LRESULT(0)
            }

            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }

            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{WheelEvent, parse_wheel_message, request_matches};

    #[test]
    fn page_messages_are_bound_to_a_valid_request() {
        assert_eq!(
            parse_wheel_message("heartbeat:5"),
            Some((5, WheelEvent::Heartbeat))
        );
        assert_eq!(
            parse_wheel_message("ready_to_show:5"),
            Some((5, WheelEvent::Ready))
        );
        assert_eq!(
            parse_wheel_message("dismiss:5"),
            Some((5, WheelEvent::Dismiss))
        );
        assert_eq!(
            parse_wheel_message("select:5:17"),
            Some((5, WheelEvent::Select(17)))
        );
        for invalid in [
            "ready_to_show",
            "heartbeat:5:6",
            "heartbeat:",
            "dismiss",
            "select:17",
            "select:5:-1",
            "select:5:7:8",
        ] {
            assert_eq!(parse_wheel_message(invalid), None);
        }
    }

    #[test]
    fn stale_or_completed_requests_cannot_publish_page_events() {
        assert!(request_matches(5, 5, true, -1));
        assert!(!request_matches(4, 5, true, -1));
        assert!(!request_matches(5, 5, false, -1));
        assert!(!request_matches(5, 5, true, 7));
    }
}
