use super::html::get_wheel_template;
use super::state::{
    HwndWrapper, IS_WARMED_UP, IS_WARMING_UP, OVERLAY_HWND, PENDING_CSS, PENDING_DISMISS_LABEL,
    PENDING_ITEMS_HTML, PENDING_POS, REGISTER_OVERLAY_CLASS, REGISTER_WHEEL_CLASS, WHEEL_ACTIVE,
    WHEEL_HEIGHT, WHEEL_HWND, WHEEL_RESULT, WHEEL_WEB_CONTEXT, WHEEL_WEBVIEW, WHEEL_WIDTH,
    WM_APP_HIDE, WM_APP_REAL_SHOW, WM_APP_SHOW,
};
use std::sync::atomic::Ordering;
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
    GetSystemMetrics, HWND_TOPMOST, IDC_ARROW, KillTimer, LWA_ALPHA, LoadCursorW, MSG,
    PostMessageW, PostQuitMessage, RegisterClassW, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN,
    SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SW_HIDE, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOMOVE,
    SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW, SetLayeredWindowAttributes, SetTimer, SetWindowPos,
    ShowWindow, TranslateMessage, WM_CLOSE, WM_DESTROY, WM_DPICHANGED, WM_ERASEBKGND, WM_KEYDOWN,
    WM_LBUTTONDOWN, WM_RBUTTONDOWN, WM_TIMER, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
};
use windows::core::w;
use wry::{Rect, WebContext, WebViewBuilder};

pub(crate) fn internal_create_window_loop() {
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
                let shared_data_dir = crate::overlay::get_shared_webview_data_dir(Some("common"));
                *ctx.borrow_mut() = Some(WebContext::new(Some(shared_data_dir)));
            }
        });

        let webview_res = {
            let _init_lock = crate::overlay::GLOBAL_WEBVIEW_MUTEX.lock().unwrap();
            crate::log_info!("[PresetWheel] Acquired init lock. Building...");

            let build_res = WHEEL_WEB_CONTEXT.with(|ctx| {
                let mut ctx_ref = ctx.borrow_mut();
                let builder = if let Some(web_ctx) = ctx_ref.as_mut() {
                    WebViewBuilder::new_with_web_context(web_ctx)
                } else {
                    WebViewBuilder::new()
                };
                let builder =
                    crate::overlay::html_components::font_manager::configure_webview(builder);

                let template_html = get_wheel_template(true);
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
                        let body = msg.body();
                        if body == "ready_to_show" {
                            let hwnd_val = WHEEL_HWND.load(Ordering::SeqCst);
                            let wheel_hwnd = HWND(hwnd_val as *mut _);
                            if !wheel_hwnd.is_invalid() {
                                let _ = PostMessageW(
                                    Some(wheel_hwnd),
                                    WM_APP_REAL_SHOW,
                                    WPARAM(0),
                                    LPARAM(0),
                                );
                            }
                        } else if body == "dismiss" {
                            hide_wheel_with_result(-2);
                        } else if let Some(idx_str) = body.strip_prefix("select:")
                            && let Ok(idx) = idx_str.parse::<i32>()
                        {
                            hide_wheel_with_result(idx);
                        }
                    })
                    .build(&wrapper)
            });
            crate::log_info!(
                "[PresetWheel] Build finished. Status: {}",
                if build_res.is_ok() { "OK" } else { "ERR" }
            );
            build_res
        };

        if let Ok(wv) = webview_res {
            WHEEL_WEBVIEW.with(|cell| {
                *cell.borrow_mut() = Some(wv);
            });
            let _ = ShowWindow(hwnd, SW_HIDE);
            WHEEL_HWND.store(hwnd.0 as isize, Ordering::SeqCst);
            IS_WARMING_UP.store(false, Ordering::SeqCst);
            IS_WARMED_UP.store(true, Ordering::SeqCst);
        } else {
            let _ = DestroyWindow(hwnd);
            let _ = DestroyWindow(overlay_hwnd);
            IS_WARMING_UP.store(false, Ordering::SeqCst);
            OVERLAY_HWND.store(0, Ordering::SeqCst);
            WHEEL_HWND.store(0, Ordering::SeqCst);
            CoUninitialize();
            return;
        }

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        WHEEL_WEBVIEW.with(|cell| {
            *cell.borrow_mut() = None;
        });
        WHEEL_HWND.store(0, Ordering::SeqCst);
        OVERLAY_HWND.store(0, Ordering::SeqCst);
        IS_WARMING_UP.store(false, Ordering::SeqCst);
        CoUninitialize();
    }
}

unsafe fn hide_wheel_with_result(result: i32) {
    let hwnd_val = WHEEL_HWND.load(Ordering::SeqCst);
    let wheel_hwnd = HWND(hwnd_val as *mut _);
    if !wheel_hwnd.is_invalid() {
        unsafe {
            let _ = PostMessageW(Some(wheel_hwnd), WM_APP_HIDE, WPARAM(0), LPARAM(0));
        }
    }
    WHEEL_RESULT.store(result, Ordering::SeqCst);
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
                hide_wheel_with_result(-2);
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
                let items_html = PENDING_ITEMS_HTML.lock().unwrap().clone();
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
                            "window.updateContent(`{}`, `{}`);",
                            items_html
                                .replace("\\", "\\\\")
                                .replace("`", "\\`")
                                .replace("$", "\\$"),
                            dismiss_label.replace("`", "\\`").replace("$", "\\$")
                        );
                        let _ = webview.evaluate_script(&script);
                        let _ = webview.set_bounds(Rect {
                            position: wry::dpi::Position::Physical(
                                wry::dpi::PhysicalPosition::new(0, 0),
                            ),
                            size: wry::dpi::Size::Physical(wry::dpi::PhysicalSize::new(
                                WHEEL_WIDTH as u32,
                                WHEEL_HEIGHT as u32,
                            )),
                        });
                    }
                });

                SetTimer(Some(hwnd), 99, 150, None);
                LRESULT(0)
            }

            WM_APP_REAL_SHOW => {
                let _ = KillTimer(Some(hwnd), 99);
                let (target_x, target_y) = *PENDING_POS.lock().unwrap();

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
                let _ = SetWindowPos(
                    hwnd,
                    Some(HWND_TOPMOST),
                    target_x,
                    target_y,
                    WHEEL_WIDTH + 50,
                    WHEEL_HEIGHT + 50,
                    SWP_NOACTIVATE,
                );

                LRESULT(0)
            }

            WM_TIMER => {
                if wparam.0 == 99 {
                    let _ = PostMessageW(Some(hwnd), WM_APP_REAL_SHOW, WPARAM(0), LPARAM(0));
                }
                LRESULT(0)
            }

            WM_APP_HIDE => {
                let _ = KillTimer(Some(hwnd), 99);
                let _ = ShowWindow(hwnd, SW_HIDE);
                let overlay_val = OVERLAY_HWND.load(Ordering::SeqCst);
                let overlay = HWND(overlay_val as *mut _);
                if !overlay.is_invalid() {
                    let _ = ShowWindow(overlay, SW_HIDE);
                }

                WHEEL_WEBVIEW.with(|wv| {
                    if let Some(webview) = wv.borrow().as_ref() {
                        let _ = webview
                            .evaluate_script("document.getElementById('grid').innerHTML = '';");
                    }
                });

                WHEEL_ACTIVE.store(false, Ordering::SeqCst);
                LRESULT(0)
            }

            WM_KEYDOWN => {
                if wparam.0 as u32 == 0x1B {
                    hide_wheel_with_result(-2);
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
