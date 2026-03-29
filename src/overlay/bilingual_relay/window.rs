use std::num::NonZeroIsize;

use raw_window_handle::{
    HandleError, HasWindowHandle, RawWindowHandle, Win32WindowHandle, WindowHandle,
};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::{
    DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmExtendFrameIntoClientArea,
    DwmSetWindowAttribute,
};
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::Graphics::Gdi::HBRUSH;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{HSTRING, PCWSTR, w};
use wry::{Rect, WebContext, WebViewBuilder};

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
                super::state::with_state(|state| {
                    state.applied = super::current_settings();
                    state.draft = state.applied.clone();
                    state.last_error = None;
                    state.hotkey_error = None;
                    state.normalize();
                });
                refresh_window_chrome(hwnd);
                super::state::sync_to_webview();
                let _ = ShowWindow(hwnd, SW_RESTORE);
                let _ = ShowWindow(hwnd, SW_SHOW);
                let _ = SetForegroundWindow(hwnd);
                let _ = SetFocus(Some(hwnd));
                super::auto_start_if_possible();
                LRESULT(0)
            }
            super::WM_APP_SYNC => {
                super::state::sync_to_webview();
                LRESULT(0)
            }
            WM_CLOSE => {
                super::runtime::stop_session();
                super::publish_connection(super::RelayConnectionState::Stopped, false, None);
                let _ = ShowWindow(hwnd, SW_HIDE);
                LRESULT(0)
            }
            WM_NCCALCSIZE => {
                if wparam.0 != 0 {
                    LRESULT(0)
                } else {
                    DefWindowProcW(hwnd, msg, wparam, lparam)
                }
            }
            WM_ERASEBKGND => LRESULT(1),
            WM_SIZE => {
                resize_webview(hwnd);
                LRESULT(0)
            }
            WM_DESTROY => {
                super::runtime::stop_session();
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

unsafe fn internal_create_loop() {
    let instance = unsafe { GetModuleHandleW(None).unwrap() };
    let class_name = w!("BilingualRelayWindowClass");

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
    let width = ((screen_w as f64 * 0.30) as i32).clamp(480, 620);
    let height = ((screen_h as f64 * 0.36) as i32).clamp(340, 440);
    let x = (screen_w - width) / 2;
    let y = (screen_h - height) / 2;

    let title = HSTRING::from(
        crate::gui::locale::LocaleText::get(&super::current_ui_language()).bilingual_relay_title,
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
    let initial_payload = super::state::payload_json().unwrap_or_else(|| "null".to_string());
    let is_dark = crate::overlay::is_dark_mode();
    let background = if is_dark {
        (17, 18, 26, 255)
    } else {
        (246, 247, 251, 255)
    };
    let theme_name = if is_dark { "dark" } else { "light" };
    let init_script = format!(
        r#"
        (function() {{
            const originalPostMessage = window.ipc.postMessage;
            window.isWry = true;
            window.__BR_INITIAL_STATE__ = {initial_payload};
            window.invoke = async (cmd, args = {{}}) => {{
                return new Promise((resolve, reject) => {{
                    const id = Math.random().toString(36).slice(2);
                    const handler = (event) => {{
                        if (event.detail && event.detail.id === id) {{
                            window.removeEventListener('ipc-reply', handler);
                            if (event.detail.error) reject(event.detail.error);
                            else resolve(event.detail.result);
                        }}
                    }};
                    window.addEventListener('ipc-reply', handler);
                    originalPostMessage(JSON.stringify({{ id, cmd, args }}));
                }});
            }};
            if (document.documentElement) document.documentElement.dataset.theme = '{theme_name}';
        }})();
        "#
    );

    // Build inlined HTML and serve via the shared font server
    // so this WebView joins the shared browser process (same user data dir + origin)
    let inlined_html = super::assets::build_inlined_html();
    let page_url =
        crate::overlay::html_components::font_manager::store_html_page(inlined_html);

    super::WEB_CONTEXT.with(|context| {
        let mut context_ref = context.borrow_mut();
        if context_ref.is_none() {
            *context_ref = Some(WebContext::new(Some(
                crate::overlay::get_shared_webview_data_dir(Some("common")),
            )));
        }

        let webview_result = {
            let _init_lock = crate::overlay::GLOBAL_WEBVIEW_MUTEX.lock().unwrap();

            let url = page_url
                .as_deref()
                .unwrap_or("about:blank");

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
    super::state::sync_to_webview();

    unsafe {
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
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
        crate::gui::locale::LocaleText::get(&super::current_ui_language()).bilingual_relay_title,
    );
    unsafe {
        let _ = SetWindowTextW(hwnd, PCWSTR(title.as_ptr()));
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

struct HwndWrapper(HWND);

impl HasWindowHandle for HwndWrapper {
    fn window_handle(&self) -> std::result::Result<WindowHandle<'_>, HandleError> {
        let hwnd = self.0.0 as isize;
        let non_zero = NonZeroIsize::new(hwnd).ok_or(HandleError::Unavailable)?;
        let mut handle = Win32WindowHandle::new(non_zero);
        handle.hinstance = None;
        Ok(unsafe { WindowHandle::borrow_raw(RawWindowHandle::Win32(handle)) })
    }
}
