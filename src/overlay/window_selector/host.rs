use std::num::NonZeroIsize;
use std::sync::Once;
use std::sync::atomic::{AtomicIsize, AtomicU8, Ordering};

use raw_window_handle::{
    HandleError, HasWindowHandle, RawWindowHandle, Win32WindowHandle, WindowHandle,
};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::DwmExtendFrameIntoClientArea;
use windows::Win32::Graphics::Gdi::HBRUSH;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::WindowsAndMessaging::*;
use wry::{Rect, WebContext, WebViewBuilder};

use super::html::generate_html;
use super::{SelectorCallbacks, SelectorEntry, SelectorOwner, SelectorText};

const WM_SELECTOR_RUN_SCRIPT: u32 = WM_APP + 1;

static REGISTER_SELECTOR_CLASS: Once = Once::new();
static SELECTOR_HWND: AtomicIsize = AtomicIsize::new(0);
static SELECTOR_OWNER: AtomicU8 = AtomicU8::new(0);

thread_local! {
    static SELECTOR_WEBVIEW: std::cell::RefCell<Option<wry::WebView>> =
        const { std::cell::RefCell::new(None) };
    static SELECTOR_WEB_CONTEXT: std::cell::RefCell<Option<WebContext>> =
        const { std::cell::RefCell::new(None) };
}

struct HwndWrapper(HWND);

impl HasWindowHandle for HwndWrapper {
    fn window_handle(&self) -> std::result::Result<WindowHandle<'_>, HandleError> {
        let hwnd = self.0.0 as isize;
        if hwnd == 0 {
            return Err(HandleError::Unavailable);
        }

        if let Some(non_zero) = NonZeroIsize::new(hwnd) {
            let mut handle = Win32WindowHandle::new(non_zero);
            handle.hinstance = None;
            let raw = RawWindowHandle::Win32(handle);
            Ok(unsafe { WindowHandle::borrow_raw(raw) })
        } else {
            Err(HandleError::Unavailable)
        }
    }
}

unsafe extern "system" fn selector_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        match msg {
            WM_SIZE => {
                SELECTOR_WEBVIEW.with(|wv| {
                    if let Some(webview) = wv.borrow().as_ref() {
                        let mut rect = RECT::default();
                        let _ = GetClientRect(hwnd, &mut rect);
                        let _ = webview.set_bounds(Rect {
                            position: wry::dpi::Position::Physical(
                                wry::dpi::PhysicalPosition::new(0, 0),
                            ),
                            size: wry::dpi::Size::Physical(wry::dpi::PhysicalSize::new(
                                (rect.right - rect.left) as u32,
                                (rect.bottom - rect.top) as u32,
                            )),
                        });
                    }
                });
                LRESULT(0)
            }
            WM_SELECTOR_RUN_SCRIPT => {
                let script_ptr = lparam.0 as *mut String;
                if !script_ptr.is_null() {
                    let script = Box::from_raw(script_ptr);
                    SELECTOR_WEBVIEW.with(|wv| {
                        if let Some(webview) = wv.borrow().as_ref() {
                            let _ = webview.evaluate_script(&script);
                        }
                    });
                }
                LRESULT(0)
            }
            WM_CLOSE => {
                let _ = DestroyWindow(hwnd);
                LRESULT(0)
            }
            WM_DESTROY => {
                SELECTOR_HWND.store(0, Ordering::SeqCst);
                SELECTOR_OWNER.store(0, Ordering::SeqCst);
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

pub fn selector_is_closed() -> bool {
    SELECTOR_HWND.load(Ordering::SeqCst) == 0
}

pub fn selector_owner() -> Option<SelectorOwner> {
    SelectorOwner::from_u8(SELECTOR_OWNER.load(Ordering::SeqCst))
}

pub fn is_owner_active(owner: SelectorOwner) -> bool {
    SELECTOR_HWND.load(Ordering::SeqCst) != 0 && selector_owner() == Some(owner)
}

pub fn close_selector() {
    let value = SELECTOR_HWND.load(Ordering::SeqCst);
    if value == 0 {
        return;
    }

    unsafe {
        let _ = PostMessageW(Some(HWND(value as *mut _)), WM_CLOSE, WPARAM(0), LPARAM(0));
    }
}

pub fn close_selector_for_owner(owner: SelectorOwner) {
    if is_owner_active(owner) {
        close_selector();
    }
}

fn post_script(script: String) {
    let value = SELECTOR_HWND.load(Ordering::SeqCst);
    if value == 0 {
        return;
    }

    let script_ptr = Box::into_raw(Box::new(script));
    unsafe {
        let _ = PostMessageW(
            Some(HWND(value as *mut _)),
            WM_SELECTOR_RUN_SCRIPT,
            WPARAM(0),
            LPARAM(script_ptr as isize),
        );
    }
}

pub fn post_preview_update_for_owner(owner: SelectorOwner, entry_id: &str, data_url: String) {
    if !is_owner_active(owner) {
        return;
    }

    let id_json = match serde_json::to_string(entry_id) {
        Ok(value) => value,
        Err(_) => return,
    };
    let url_json = match serde_json::to_string(&data_url) {
        Ok(value) => value,
        Err(_) => return,
    };

    post_script(format!("window.updateThumb({id_json}, {url_json});"));
}

pub fn update_theme(is_dark: bool) {
    if selector_is_closed() {
        return;
    }

    let theme = if is_dark { "dark" } else { "light" };
    post_script(format!("window.setTheme('{theme}');"));
}

pub fn show_selector(
    owner: SelectorOwner,
    entries: Vec<SelectorEntry>,
    is_dark: bool,
    text: SelectorText,
    callbacks: SelectorCallbacks,
) {
    if entries.is_empty() {
        return;
    }

    if !selector_is_closed() {
        close_selector();
        std::thread::sleep(std::time::Duration::from_millis(80));
    }

    std::thread::spawn(move || unsafe {
        let hinstance = match GetModuleHandleW(None) {
            Ok(value) => value,
            Err(_) => return,
        };

        REGISTER_SELECTOR_CLASS.call_once(|| {
            let _ = RegisterClassExW(&WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(selector_wnd_proc),
                hInstance: hinstance.into(),
                lpszClassName: windows::core::w!("SharedWindowSelectorClass"),
                hbrBackground: HBRUSH(std::ptr::null_mut()),
                ..Default::default()
            });
        });

        let screen_x = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let screen_y = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let screen_w = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let screen_h = GetSystemMetrics(SM_CYVIRTUALSCREEN);

        let hwnd = match CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            windows::core::w!("SharedWindowSelectorClass"),
            windows::core::w!(""),
            WS_POPUP | WS_VISIBLE,
            screen_x,
            screen_y,
            screen_w,
            screen_h,
            None,
            None,
            Some(hinstance.into()),
            None,
        ) {
            Ok(value) => value,
            Err(error) => {
                eprintln!("[WindowSelector] CreateWindowExW failed: {error}");
                return;
            }
        };

        let margins = MARGINS {
            cxLeftWidth: -1,
            cxRightWidth: -1,
            cyTopHeight: -1,
            cyBottomHeight: -1,
        };
        let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);

        SELECTOR_HWND.store(hwnd.0 as isize, Ordering::SeqCst);
        SELECTOR_OWNER.store(owner.as_u8(), Ordering::SeqCst);

        SELECTOR_WEB_CONTEXT.with(|context| {
            if context.borrow().is_none() {
                let shared_data_dir = crate::overlay::get_shared_webview_data_dir(Some("common"));
                *context.borrow_mut() = Some(WebContext::new(Some(shared_data_dir)));
            }
        });

        let font_css = crate::overlay::html_components::font_manager::get_font_css();
        let html = generate_html(&entries, &font_css, is_dark, &text);
        let page_url = crate::overlay::html_components::font_manager::store_html_page(html.clone())
            .unwrap_or_else(|| format!("data:text/html,{}", urlencoding::encode(&html)));

        let wrapper = HwndWrapper(hwnd);
        let on_select = callbacks.on_select.clone();
        let on_cancel = callbacks.on_cancel.clone();
        let webview_result = {
            let _lock = crate::overlay::GLOBAL_WEBVIEW_MUTEX.lock().unwrap();
            SELECTOR_WEB_CONTEXT.with(|ctx_cell| {
                let mut ctx_ref = ctx_cell.borrow_mut();
                let builder = WebViewBuilder::new_with_web_context(ctx_ref.as_mut().unwrap());
                let builder =
                    crate::overlay::html_components::font_manager::configure_webview(builder);
                builder
                    .with_transparent(true)
                    .with_background_color((0, 0, 0, 0))
                    .with_url(&page_url)
                    .with_ipc_handler(move |msg: wry::http::Request<String>| {
                        let body = msg.body().to_string();
                        if let Some(entry_id) = body.strip_prefix("select:") {
                            on_select(entry_id.to_string());
                        } else if body == "cancel" {
                            on_cancel();
                        }
                        close_selector();
                    })
                    .build_as_child(&wrapper)
            })
        };

        let webview = match webview_result {
            Ok(value) => value,
            Err(error) => {
                eprintln!("[WindowSelector] WebView build failed: {error}");
                SELECTOR_HWND.store(0, Ordering::SeqCst);
                SELECTOR_OWNER.store(0, Ordering::SeqCst);
                let _ = DestroyWindow(hwnd);
                return;
            }
        };

        let _ = webview.set_bounds(Rect {
            position: wry::dpi::Position::Physical(wry::dpi::PhysicalPosition::new(0, 0)),
            size: wry::dpi::Size::Physical(wry::dpi::PhysicalSize::new(
                screen_w as u32,
                screen_h as u32,
            )),
        });

        SELECTOR_WEBVIEW.with(|wv| {
            *wv.borrow_mut() = Some(webview);
        });

        let mut msg = MSG::default();
        loop {
            match GetMessageW(&mut msg, None, 0, 0).0 {
                -1 | 0 => break,
                _ => {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        }

        SELECTOR_WEBVIEW.with(|wv| {
            *wv.borrow_mut() = None;
        });
        SELECTOR_WEB_CONTEXT.with(|ctx| {
            *ctx.borrow_mut() = None;
        });
    });
}
