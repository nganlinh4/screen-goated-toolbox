use crate::win_types::HwndWrapper;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::{
    DWMWA_EXTENDED_FRAME_BOUNDS, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
    DwmExtendFrameIntoClientArea, DwmGetWindowAttribute, DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Gdi::HBRUSH;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{HSTRING, PCWSTR, w};
use wry::WebViewBuilderExtWindows;
use wry::{DragDropEvent, Rect, WebViewBuilder};

fn is_supported_image(path: &std::path::Path) -> bool {
    crate::overlay::creation_file_dialogs::is_supported_image_path(path)
}

fn set_file_drag_active(active: bool) {
    super::WEBVIEW.with(|slot| {
        if let Some(webview) = slot.borrow().as_ref() {
            let _ = webview.evaluate_script(&format!(
                "window.handleNativeFileDrag?.({});",
                if active { "true" } else { "false" }
            ));
        }
    });
}

fn handle_file_drop(event: DragDropEvent) -> bool {
    match event {
        DragDropEvent::Enter { paths, .. } => {
            set_file_drag_active(paths.iter().any(|path| is_supported_image(path)));
        }
        DragDropEvent::Drop { paths, .. } => {
            set_file_drag_active(false);
            let paths = crate::overlay::creation_file_dialogs::admit_image_paths(
                paths,
                crate::overlay::creation_file_dialogs::MAX_BATCH_IMAGES,
                crate::overlay::creation_file_dialogs::MAX_BATCH_BYTES,
            )
            .unwrap_or_default()
            .into_iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
            if !paths.is_empty()
                && let Ok(payload) = serde_json::to_string(&paths)
            {
                super::WEBVIEW.with(|slot| {
                    if let Some(webview) = slot.borrow().as_ref() {
                        let _ = webview
                            .evaluate_script(&format!("window.handleNativeFileDrop?.({payload});"));
                    }
                });
            }
        }
        DragDropEvent::Leave => set_file_drag_active(false),
        _ => {}
    }
    true
}

fn test_launch_stays_hidden() -> bool {
    std::env::var_os("SGT_CREATION_WEBVIEW2_DEBUG_PORT").is_some()
        && std::env::var("SGT_CREATION_UI_TEST_HIDDEN").as_deref() == Ok("1")
}

pub(super) fn show() {
    if let Some(hwnd) = super::WINDOW.ready_hwnd() {
        if !test_launch_stays_hidden() {
            unsafe {
                let _ = PostMessageW(Some(hwnd), super::WM_APP_SHOW, WPARAM(0), LPARAM(0));
            }
        }
        return;
    }
    if let Some(generation) = super::WINDOW.try_claim() {
        crate::overlay::creation_close::reset_product("3d");
        std::thread::spawn(move || unsafe { internal_create_loop(generation) });
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        match msg {
            super::WM_APP_SHOW => {
                refresh_window_chrome(hwnd);
                let _ = ShowWindow(hwnd, SW_RESTORE);
                let _ = ShowWindow(hwnd, SW_SHOW);
                let _ = SetForegroundWindow(hwnd);
                let _ = SetFocus(Some(hwnd));
                LRESULT(0)
            }
            super::WM_APP_SYNC => {
                refresh_window_chrome(hwnd);
                super::WEBVIEW.with(|slot| {
                    if let Some(webview) = slot.borrow().as_ref() {
                        let _ = webview.evaluate_script(
                            "window.dispatchEvent(new Event('creation-verification-changed'))",
                        );
                    }
                });
                LRESULT(0)
            }
            super::WM_APP_PREVIEW_REPLY => {
                super::ipc::flush_preview_replies(hwnd);
                LRESULT(0)
            }
            super::WM_APP_IPC_REPLY => {
                super::ipc::flush_replies(hwnd);
                LRESULT(0)
            }
            WM_NCCALCSIZE => {
                if wparam.0 != 0 {
                    LRESULT(0)
                } else {
                    DefWindowProcW(hwnd, msg, wparam, lparam)
                }
            }
            WM_NCHITTEST => resize_hit_test(hwnd, lparam),
            WM_GETMINMAXINFO => {
                let info = &mut *(lparam.0 as *mut MINMAXINFO);
                let area = crate::overlay::display_placement::window_work_area(hwnd);
                let (width, height) =
                    super::window_geometry::minimum(area, super::window_geometry::dpi(hwnd));
                info.ptMinTrackSize.x = width;
                info.ptMinTrackSize.y = height;
                LRESULT(0)
            }
            WM_ENTERSIZEMOVE => {
                super::window_geometry::begin_user_resize(hwnd);
                LRESULT(0)
            }
            WM_SIZING => {
                super::window_geometry::mark_user_resize();
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_EXITSIZEMOVE => {
                super::window_geometry::finish_user_resize(hwnd);
                LRESULT(0)
            }
            WM_DPICHANGED => {
                crate::overlay::display_placement::apply_suggested_dpi_rect(hwnd, lparam);
                LRESULT(0)
            }
            WM_ERASEBKGND => LRESULT(1),
            WM_SIZE => {
                resize_webview(hwnd);
                LRESULT(0)
            }
            WM_CLOSE => {
                if wparam.0 != 0 && !crate::overlay::creation_close::accept_retry(hwnd, wparam.0) {
                    return LRESULT(0);
                }
                crate::overlay::creation_preview::unregister_async_target(hwnd);
                super::ipc_worker::unregister(hwnd);
                super::asset_protocol::clear();
                if super::shutdown() {
                    crate::overlay::creation_close::clear(hwnd);
                    let _ = DestroyWindow(hwnd);
                } else {
                    crate::log_info!("[3D Generator] Close is waiting for durable cancellation.");
                    crate::overlay::creation_close::schedule(hwnd);
                }
                LRESULT(0)
            }
            super::WM_APP_REMOVE => {
                crate::overlay::creation_preview::unregister_async_target(hwnd);
                super::ipc_worker::unregister(hwnd);
                super::asset_protocol::clear();
                if super::shutdown() {
                    let _ = DestroyWindow(hwnd);
                }
                LRESULT(0)
            }
            WM_DESTROY => {
                crate::overlay::creation_close::clear(hwnd);
                crate::overlay::creation_preview::unregister_async_target(hwnd);
                super::ipc_worker::unregister(hwnd);
                super::asset_protocol::clear();
                if super::WINDOW.is_ready_window(hwnd) {
                    let _ = super::shutdown();
                }
                super::WEBVIEW.with(|webview| {
                    *webview.borrow_mut() = None;
                });
                super::ASSET_PACK.with(|slot| *slot.borrow_mut() = None);
                super::WINDOW.release_window(hwnd);
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

unsafe fn internal_create_loop(generation: u64) {
    let instance = unsafe { GetModuleHandleW(None).unwrap() };
    let class_name = w!("ThreeDGeneratorWindowClass");

    super::REGISTER_CLASS.call_once(|| unsafe {
        let wc = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance.into(),
            lpszClassName: class_name,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap(),
            hbrBackground: HBRUSH(std::ptr::null_mut()),
            ..Default::default()
        };
        let _ = RegisterClassW(&wc);
    });

    let area = crate::overlay::display_placement::primary_work_area();
    let screen_w = area.width;
    let screen_h = area.height;
    // Establish an HWND first so its monitor DPI can be queried reliably.
    let rect = area.centered(screen_w.min(920), screen_h.min(600));

    let title = HSTRING::from(
        crate::gui::locale::LocaleText::get(&super::current_ui_language())
            .shell
            .three_d_generator_title,
    );

    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_APPWINDOW,
            class_name,
            PCWSTR(title.as_ptr()),
            WS_POPUP | WS_THICKFRAME | WS_MINIMIZEBOX | WS_SYSMENU,
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .unwrap_or_default()
    };

    if hwnd.is_invalid() {
        super::WINDOW.release(generation);
        return;
    }

    let rect = super::window_geometry::initial(hwnd, area);
    let _ = unsafe {
        SetWindowPos(
            hwnd,
            None,
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
            SWP_NOZORDER | SWP_NOACTIVATE,
        )
    };

    if !super::WINDOW.publish_hwnd(generation, hwnd) {
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
        return;
    }
    crate::overlay::creation_preview::register_async_target(hwnd);
    super::ipc_worker::register(hwnd);

    let corner_pref = DWMWCP_ROUND;
    let _ = unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &corner_pref as *const _ as *const std::ffi::c_void,
            std::mem::size_of_val(&corner_pref) as u32,
        )
    };
    let margins = MARGINS {
        cxLeftWidth: -1,
        cxRightWidth: -1,
        cyTopHeight: -1,
        cyBottomHeight: -1,
    };
    let _ = unsafe { DwmExtendFrameIntoClientArea(hwnd, &margins) };
    refresh_window_chrome(hwnd);

    let wrapper = HwndWrapper(hwnd);
    let is_dark = crate::overlay::is_dark_mode();
    let background = if is_dark {
        (30, 30, 46, 255)
    } else {
        (244, 248, 247, 255)
    };
    let theme_name = if is_dark { "dark" } else { "light" };
    let language = super::current_ui_language();
    let host_context = serde_json::json!({ "theme": theme_name, "language": language });
    let init_script = format!(
        r#"
        (function() {{
            const originalPostMessage = window.ipc.postMessage;
            window.isWry = true;
            window.invoke = async (cmd, args = {{}}) => {{
                return new Promise((resolve, reject) => {{
                    const id = Math.random().toString(36).slice(2);
                    const handler = (event) => {{
                        if (event.detail && event.detail.id === id) {{
                            window.removeEventListener('ipc-reply', handler);
                            if (event.detail.error) reject(new Error(event.detail.error));
                            else resolve(event.detail.result);
                        }}
                    }};
                    window.addEventListener('ipc-reply', handler);
                    originalPostMessage(JSON.stringify({{ id, cmd, args }}));
                }});
            }};
            window.__SGT_CONTEXT__ = {host_context};
            if (document.documentElement) {{
                document.documentElement.dataset.theme = window.__SGT_CONTEXT__.theme;
                document.documentElement.lang = window.__SGT_CONTEXT__.language;
            }}
        }})();
        "#
    );

    let (inlined_html, web_asset_pack) = match super::assets::build_inlined_html() {
        Ok(assets) => assets,
        Err(error) => {
            crate::log_info!("[3D Generator] Could not open interface assets: {error}");
            crate::overlay::creation_preview::unregister_async_target(hwnd);
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
            super::WINDOW.release(generation);
            return;
        }
    };
    let page_url = crate::overlay::html_components::font_manager::store_html_page(inlined_html);

    let webview_result = super::WEB_CONTEXT.with(|context| {
        let mut context_ref = context.borrow_mut();
        if context_ref.is_none() {
            *context_ref = Some(crate::overlay::webview_runtime::create_context(
                crate::overlay::webview_runtime::creation_profile(),
            ));
        }

        let webview_result = {
            let init = crate::overlay::webview_init::acquire("three-d-generator");
            let url = page_url.as_deref().unwrap_or("about:blank");
            let allowed_navigation_url = url.to_string();
            let allowed_ipc_url = url.to_string();
            let mut builder = WebViewBuilder::new_with_web_context(context_ref.as_mut().unwrap())
                .with_background_color(background)
                .with_asynchronous_custom_protocol("sgt3d".to_string(), |_id, request, responder| {
                    super::asset_protocol_async::request(request, responder)
                })
                .with_initialization_script(&init_script)
                .with_ipc_handler(move |request: wry::http::Request<String>| {
                    if is_allowed_page(&allowed_ipc_url, &request.uri().to_string()) {
                        super::ipc::handle_ipc(hwnd, request.body());
                    }
                })
                .with_navigation_handler(move |requested| {
                    is_allowed_page(&allowed_navigation_url, &requested)
                })
                .with_drag_drop_handler(handle_file_drop)
                .with_url(url);
            if let Some((port, arguments)) =
                crate::overlay::webview_runtime::creation_debug_browser_args()
            {
                builder = builder.with_additional_browser_args(arguments);
                crate::log_info!(
                    "[CreationWebView] product=three-d-generator test debugging enabled on port {port}"
                );
            }
            let result = builder.build_as_child(&wrapper);
            init.finish(result.is_ok());
            result
        };

        let webview = webview_result?;
        crate::overlay::webview_diagnostics::attach_webview2_diagnostics(
            "three-d-generator",
            hwnd,
            &webview,
        );
        super::WEBVIEW.with(|slot| *slot.borrow_mut() = Some(webview));
        super::ASSET_PACK.with(|slot| *slot.borrow_mut() = Some(web_asset_pack));
        Ok::<(), wry::Error>(())
    });
    if let Err(error) = webview_result {
        crate::log_info!("[3D Generator] Could not create interface: {error}");
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
        super::WINDOW.release(generation);
        super::WEB_CONTEXT.with(|context| *context.borrow_mut() = None);
        return;
    }

    resize_webview(hwnd);

    unsafe {
        let _ = ShowWindow(hwnd, SW_HIDE);
    }
    super::WINDOW.mark_ready(generation);
    if let Some(message) = ready_action(
        super::REMOVAL_REQUESTED.load(std::sync::atomic::Ordering::Acquire),
        test_launch_stays_hidden(),
    ) {
        unsafe {
            let _ = PostMessageW(Some(hwnd), message, WPARAM(0), LPARAM(0));
        }
    }

    unsafe {
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        super::WEBVIEW.with(|webview| {
            *webview.borrow_mut() = None;
        });
        super::WEB_CONTEXT.with(|context| {
            *context.borrow_mut() = None;
        });
        super::ASSET_PACK.with(|slot| *slot.borrow_mut() = None);
    }
    super::WINDOW.release(generation);
}

/// The message a window posts to itself once its WebView finished building,
/// however long that took: removal wins, a hidden test launch stays hidden and
/// every other first open becomes visible.
fn ready_action(removal_requested: bool, hidden_test_launch: bool) -> Option<u32> {
    if removal_requested {
        Some(super::WM_APP_REMOVE)
    } else if hidden_test_launch {
        None
    } else {
        Some(super::WM_APP_SHOW)
    }
}

fn is_allowed_page(expected: &str, requested: &str) -> bool {
    requested.split('#').next() == Some(expected)
}

fn refresh_window_chrome(hwnd: HWND) {
    let is_dark = crate::overlay::is_dark_mode();
    crate::gui::utils::set_window_icon(hwnd, is_dark);
    let title = HSTRING::from(
        crate::gui::locale::LocaleText::get(&super::current_ui_language())
            .shell
            .three_d_generator_title,
    );
    unsafe {
        let _ = SetWindowTextW(hwnd, PCWSTR(title.as_ptr()));
    }
    let context = serde_json::json!({
        "theme": if is_dark { "dark" } else { "light" },
        "language": super::current_ui_language(),
    });
    super::WEBVIEW.with(|slot| {
        if let Some(webview) = slot.borrow().as_ref() {
            let _ = webview.evaluate_script(&format!("window.applyHostContext?.({context});"));
        }
    });
}

unsafe fn resize_hit_test(hwnd: HWND, lparam: LPARAM) -> LRESULT {
    unsafe {
        let x = lparam.0 as i16 as i32;
        let y = (lparam.0 >> 16) as i16 as i32;
        let mut frame = RECT::default();
        let _ = GetWindowRect(hwnd, &mut frame);
        let _ = DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut frame as *mut _ as *mut std::ffi::c_void,
            std::mem::size_of::<RECT>() as u32,
        );
        let border = super::window_geometry::border(hwnd);
        let left = frame.left + border;
        let right = frame.right - border;
        let top = frame.top + border;
        let bottom = frame.bottom - border;
        if y < top {
            return LRESULT(if x < left {
                HTTOPLEFT
            } else if x > right {
                HTTOPRIGHT
            } else {
                HTTOP
            } as isize);
        }
        if y > bottom {
            return LRESULT(if x < left {
                HTBOTTOMLEFT
            } else if x > right {
                HTBOTTOMRIGHT
            } else {
                HTBOTTOM
            } as isize);
        }
        if x < left {
            return LRESULT(HTLEFT as isize);
        }
        if x > right {
            return LRESULT(HTRIGHT as isize);
        }
        LRESULT(HTCLIENT as isize)
    }
}

fn resize_webview(hwnd: HWND) {
    super::WEBVIEW.with(|webview| {
        if let Some(webview) = webview.borrow().as_ref() {
            unsafe {
                let mut rect = RECT::default();
                let _ = GetClientRect(hwnd, &mut rect);
                let _ = webview.set_bounds(Rect {
                    position: wry::dpi::Position::Physical(wry::dpi::PhysicalPosition::new(0, 0)),
                    size: wry::dpi::Size::Physical(wry::dpi::PhysicalSize::new(
                        (rect.right - rect.left) as u32,
                        (rect.bottom - rect.top) as u32,
                    )),
                });
            }
        }
    });
}

#[cfg(test)]
mod navigation_tests {
    use super::is_allowed_page;

    #[test]
    fn a_slow_first_open_is_shown_once_its_webview_is_ready() {
        assert_eq!(
            super::ready_action(false, false),
            Some(super::super::WM_APP_SHOW)
        );
        assert_eq!(
            super::ready_action(true, false),
            Some(super::super::WM_APP_REMOVE)
        );
        assert_eq!(
            super::ready_action(true, true),
            Some(super::super::WM_APP_REMOVE)
        );
        assert_eq!(super::ready_action(false, true), None);
    }

    #[test]
    fn only_the_exact_host_page_and_its_fragments_are_trusted() {
        let url = "http://127.0.0.1:12345/page/secret";
        assert!(is_allowed_page(url, url));
        assert!(is_allowed_page(url, &format!("{url}#tab")));
        assert!(!is_allowed_page(url, &format!("{url}?other")));
        assert!(!is_allowed_page(url, "https://example.com/"));
        assert!(!is_allowed_page(
            url,
            "http://127.0.0.1:12345/page/secret/other"
        ));
    }
}
