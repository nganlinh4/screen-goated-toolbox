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
use wry::{Rect, WebContext, WebViewBuilder};

const MIN_WINDOW_WIDTH: i32 = 840;
const MIN_WINDOW_HEIGHT: i32 = 540;

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SavedWindowSize {
    width: i32,
    height: i32,
}

pub(super) fn show() {
    unsafe {
        if !super::IS_READY {
            if !super::IS_INITIALIZING {
                super::IS_INITIALIZING = true;
                std::thread::spawn(internal_create_loop_entry);
            }
            std::thread::spawn(|| {
                for _ in 0..100 {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    let hwnd = std::ptr::addr_of!(super::WINDOW_HWND).read();
                    if super::IS_READY && !hwnd.is_invalid() {
                        let _ =
                            PostMessageW(Some(hwnd.0), super::WM_APP_SHOW, WPARAM(0), LPARAM(0));
                        return;
                    }
                }
            });
            return;
        }
        let hwnd = std::ptr::addr_of!(super::WINDOW_HWND).read();
        if !hwnd.is_invalid() {
            let _ = PostMessageW(Some(hwnd.0), super::WM_APP_SHOW, WPARAM(0), LPARAM(0));
        }
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
                info.ptMinTrackSize.x = MIN_WINDOW_WIDTH;
                info.ptMinTrackSize.y = MIN_WINDOW_HEIGHT;
                LRESULT(0)
            }
            WM_EXITSIZEMOVE => {
                save_window_size(hwnd);
                LRESULT(0)
            }
            WM_ERASEBKGND => LRESULT(1),
            WM_SIZE => {
                resize_webview(hwnd);
                LRESULT(0)
            }
            WM_CLOSE => {
                save_window_size(hwnd);
                let _ = ShowWindow(hwnd, SW_HIDE);
                LRESULT(0)
            }
            WM_DESTROY => {
                super::WEBVIEW.with(|webview| {
                    *webview.borrow_mut() = None;
                });
                super::WINDOW_HWND = crate::win_types::SendHwnd(HWND(std::ptr::null_mut()));
                super::IS_READY = false;
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

unsafe fn internal_create_loop() {
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

    let screen_w = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let screen_h = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    let default_width = ((screen_w as f64 * 0.58) as i32).clamp(920, 1240);
    let default_height = ((screen_h as f64 * 0.62) as i32).clamp(600, 820);
    let saved = load_window_size().unwrap_or(SavedWindowSize {
        width: default_width,
        height: default_height,
    });
    let width = saved
        .width
        .clamp(MIN_WINDOW_WIDTH, screen_w.max(MIN_WINDOW_WIDTH));
    let height = saved
        .height
        .clamp(MIN_WINDOW_HEIGHT, screen_h.max(MIN_WINDOW_HEIGHT));
    let x = (screen_w - width) / 2;
    let y = (screen_h - height) / 2;

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
            x,
            y,
            width,
            height,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .unwrap_or_default()
    };

    if hwnd.is_invalid() {
        unsafe {
            super::IS_INITIALIZING = false;
        }
        return;
    }

    unsafe {
        super::WINDOW_HWND = crate::win_types::SendHwnd(hwnd);
    }

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

    let inlined_html = super::assets::build_inlined_html();
    let page_url = crate::overlay::html_components::font_manager::store_html_page(inlined_html);

    super::WEB_CONTEXT.with(|context| {
        let mut context_ref = context.borrow_mut();
        if context_ref.is_none() {
            *context_ref = Some(WebContext::new(Some(
                crate::overlay::get_shared_webview_data_dir(Some("common")),
            )));
        }

        let webview_result = {
            let _init_lock = crate::overlay::GLOBAL_WEBVIEW_MUTEX.lock().unwrap();
            let url = page_url.as_deref().unwrap_or("about:blank");
            let mut builder = WebViewBuilder::new_with_web_context(context_ref.as_mut().unwrap())
                .with_background_color(background)
                .with_initialization_script(&init_script)
                .with_ipc_handler(move |request: wry::http::Request<String>| {
                    super::ipc::handle_ipc(hwnd, request.body());
                })
                .with_url(url);
            builder = crate::overlay::html_components::font_manager::configure_webview(builder);
            builder.build_as_child(&wrapper)
        };

        if let Ok(webview) = webview_result {
            super::WEBVIEW.with(|slot| *slot.borrow_mut() = Some(webview));
        }
    });

    resize_webview(hwnd);

    unsafe {
        let _ = ShowWindow(hwnd, SW_HIDE);
        super::IS_READY = true;
        super::IS_INITIALIZING = false;
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
        super::WINDOW_HWND = crate::win_types::SendHwnd(HWND(std::ptr::null_mut()));
        super::IS_READY = false;
        super::IS_INITIALIZING = false;
    }
}

fn internal_create_loop_entry() {
    unsafe {
        internal_create_loop();
    }
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

fn window_size_path() -> std::path::PathBuf {
    crate::paths::app_local_data_dir().join("3d-generator-window.json")
}

fn load_window_size() -> Option<SavedWindowSize> {
    let value = std::fs::read_to_string(window_size_path()).ok()?;
    serde_json::from_str(&value).ok()
}

unsafe fn save_window_size(hwnd: HWND) {
    unsafe {
        if IsIconic(hwnd).as_bool() || IsZoomed(hwnd).as_bool() {
            return;
        }
        let mut rect = RECT::default();
        if GetWindowRect(hwnd, &mut rect).is_err() {
            return;
        }
        let size = SavedWindowSize {
            width: (rect.right - rect.left).max(MIN_WINDOW_WIDTH),
            height: (rect.bottom - rect.top).max(MIN_WINDOW_HEIGHT),
        };
        if let Ok(contents) = serde_json::to_string(&size) {
            let path = window_size_path();
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(path, contents);
        }
    }
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
        let border = 7;
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
