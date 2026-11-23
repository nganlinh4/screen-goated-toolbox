use std::sync::Once;
use std::sync::atomic::{AtomicIsize, AtomicU8, AtomicU64, Ordering};

use crate::win_types::HwndWrapper;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::DwmExtendFrameIntoClientArea;
use windows::Win32::Graphics::Gdi::HBRUSH;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::WindowsAndMessaging::*;
use wry::{Rect, WebViewBuilder};

use super::html::generate_html;
use super::script_queue::with_scripts;
use super::{SelectorCallbacks, SelectorEntry, SelectorOwner, SelectorText};

const WM_SELECTOR_RUN_SCRIPT: u32 = WM_APP + 1;

static REGISTER_SELECTOR_CLASS: Once = Once::new();
static SELECTOR_HWND: AtomicIsize = AtomicIsize::new(0);
static SELECTOR_OWNER: AtomicU8 = AtomicU8::new(0);
static SELECTOR_TOKEN: AtomicU64 = AtomicU64::new(0);
static SELECTOR_INSTANCE: AtomicU64 = AtomicU64::new(0);
static NEXT_INSTANCE: AtomicU64 = AtomicU64::new(1);
static LATEST_REQUEST: AtomicU64 = AtomicU64::new(0);
static SELECTOR_REGISTRATION: std::sync::Mutex<()> = std::sync::Mutex::new(());
type Admission = std::sync::Arc<dyn Fn(&mut dyn FnMut()) -> bool + Send + Sync>;

pub(crate) struct SelectorGuard {
    pub valid: Option<std::sync::Arc<dyn Fn() -> bool + Send + Sync>>,
    pub admission: Option<Admission>,
    pub token: u64,
}

fn admit(admission: &Option<Admission>, effect: &mut dyn FnMut()) -> bool {
    if let Some(admission) = admission {
        admission(effect)
    } else {
        effect();
        true
    }
}

fn close_matches(message_instance: usize, window_instance: usize) -> bool {
    message_instance == 0 || message_instance == window_instance
}

thread_local! {
    static SELECTOR_WEBVIEW: std::cell::RefCell<Option<wry::WebView>> =
        const { std::cell::RefCell::new(None) };
    static SELECTOR_WEB_CONTEXT: std::cell::RefCell<Option<crate::overlay::webview_runtime::ManagedContext>> =
        const { std::cell::RefCell::new(None) };
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
                for (token, script) in with_scripts(|queue| queue.take(hwnd.0 as isize)) {
                    if token != 0
                        && (SELECTOR_HWND.load(Ordering::SeqCst) != hwnd.0 as isize
                            || SELECTOR_TOKEN.load(Ordering::SeqCst) != token)
                    {
                        continue;
                    }
                    SELECTOR_WEBVIEW.with(|wv| {
                        if let Some(webview) = wv.borrow().as_ref() {
                            let _ = webview.evaluate_script(&script);
                        }
                    });
                }
                LRESULT(0)
            }
            WM_CLOSE => {
                if !close_matches(wparam.0, GetWindowLongPtrW(hwnd, GWLP_USERDATA) as usize) {
                    return LRESULT(0);
                }
                let _ = DestroyWindow(hwnd);
                LRESULT(0)
            }
            WM_DESTROY => {
                let _registration = SELECTOR_REGISTRATION
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                if SELECTOR_HWND
                    .compare_exchange(hwnd.0 as isize, 0, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
                {
                    SELECTOR_OWNER.store(0, Ordering::SeqCst);
                    SELECTOR_TOKEN.store(0, Ordering::SeqCst);
                    SELECTOR_INSTANCE.store(0, Ordering::SeqCst);
                }
                with_scripts(|queue| queue.discard(hwnd.0 as isize));
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

#[cfg(not(feature = "recorder-worker"))]
fn selector_is_closed() -> bool {
    SELECTOR_HWND.load(Ordering::SeqCst) == 0
}

pub fn selector_owner() -> Option<SelectorOwner> {
    SelectorOwner::from_u8(SELECTOR_OWNER.load(Ordering::SeqCst))
}

pub fn is_owner_active(owner: SelectorOwner) -> bool {
    let _registration = SELECTOR_REGISTRATION
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    SELECTOR_HWND.load(Ordering::SeqCst) != 0 && selector_owner() == Some(owner)
}

#[cfg(not(feature = "recorder-worker"))]
pub(crate) fn close_selector_for_token(owner: SelectorOwner, token: u64) {
    let (value, instance) = {
        let _registration = SELECTOR_REGISTRATION
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if selector_owner() != Some(owner) || SELECTOR_TOKEN.load(Ordering::SeqCst) != token {
            return;
        }
        (
            SELECTOR_HWND.load(Ordering::SeqCst),
            SELECTOR_INSTANCE.load(Ordering::SeqCst),
        )
    };
    if value != 0 {
        unsafe {
            let _ = PostMessageW(
                Some(HWND(value as *mut _)),
                WM_CLOSE,
                WPARAM(instance as usize),
                LPARAM(0),
            );
        }
    }
}

fn post_script(script: String) {
    let value = SELECTOR_HWND.load(Ordering::SeqCst);
    if value != 0 {
        queue_script(value, 0, script);
    }
}

/// Queues `script` for the selector window `value` and wakes it. A token of 0
/// runs unconditionally; otherwise the window must still own that token.
fn queue_script(value: isize, token: u64, script: String) {
    with_scripts(|queue| queue.push(value, token, script));
    let posted = unsafe {
        PostMessageW(
            Some(HWND(value as *mut _)),
            WM_SELECTOR_RUN_SCRIPT,
            WPARAM(0),
            LPARAM(0),
        )
    };
    if posted.is_err() {
        with_scripts(|queue| queue.discard(value));
    }
}

#[cfg(feature = "recorder-worker")]
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

#[cfg(not(feature = "recorder-worker"))]
pub(crate) fn post_preview_update_for_token(
    owner: SelectorOwner,
    token: u64,
    entry_id: &str,
    data_url: String,
) {
    let value = {
        let _registration = SELECTOR_REGISTRATION
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if selector_owner() != Some(owner) || SELECTOR_TOKEN.load(Ordering::SeqCst) != token {
            return;
        }
        SELECTOR_HWND.load(Ordering::SeqCst)
    };
    if value == 0 {
        return;
    }
    let (Ok(id), Ok(url)) = (
        serde_json::to_string(entry_id),
        serde_json::to_string(&data_url),
    ) else {
        return;
    };
    queue_script(value, token, format!("window.updateThumb({id}, {url});"));
}

#[cfg(not(feature = "recorder-worker"))]
pub fn update_theme(is_dark: bool) {
    if selector_is_closed() {
        return;
    }

    let theme = if is_dark { "dark" } else { "light" };
    post_script(format!("window.setTheme('{theme}');"));
}

#[cfg(feature = "recorder-worker")]
pub fn show_selector(
    owner: SelectorOwner,
    entries: Vec<SelectorEntry>,
    is_dark: bool,
    text: SelectorText,
    callbacks: SelectorCallbacks,
) {
    show_selector_impl(
        owner,
        entries,
        is_dark,
        text,
        callbacks,
        SelectorGuard {
            valid: None,
            admission: None,
            token: 0,
        },
    );
}

#[cfg(not(feature = "recorder-worker"))]
pub(crate) fn show_selector_guarded(
    owner: SelectorOwner,
    entries: Vec<SelectorEntry>,
    is_dark: bool,
    text: SelectorText,
    callbacks: SelectorCallbacks,
    guard: SelectorGuard,
) {
    show_selector_impl(owner, entries, is_dark, text, callbacks, guard);
}

fn show_selector_impl(
    owner: SelectorOwner,
    entries: Vec<SelectorEntry>,
    is_dark: bool,
    text: SelectorText,
    callbacks: SelectorCallbacks,
    guard: SelectorGuard,
) {
    let SelectorGuard {
        valid,
        admission,
        token,
    } = guard;
    let is_valid = || valid.as_ref().is_none_or(|check| check());
    if !is_valid() {
        return;
    }
    let capability = crate::runtime_support::require_webview2(
        crate::runtime_support::WebViewFeature::WindowSelector,
    );
    if !capability.is_supported() {
        crate::runtime_support::notify_capability_issue(&capability);
        return;
    }

    if entries.is_empty() {
        return;
    }

    let mut old = None;
    let mut instance = 0;
    if !admit(&admission, &mut || {
        let _registration = SELECTOR_REGISTRATION
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let Ok(next) = NEXT_INSTANCE.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |v| {
            (v < isize::MAX as u64).then_some(v + 1)
        }) else {
            return;
        };
        instance = next;
        LATEST_REQUEST.store(instance, Ordering::SeqCst);
        let value = SELECTOR_HWND.load(Ordering::SeqCst);
        if value != 0 {
            old = Some((value, SELECTOR_INSTANCE.load(Ordering::SeqCst)));
        }
    }) || instance == 0
    {
        return;
    }
    if let Some((value, instance)) = old {
        unsafe {
            let _ = PostMessageW(
                Some(HWND(value as *mut _)),
                WM_CLOSE,
                WPARAM(instance as usize),
                LPARAM(0),
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(80));
    }

    if !is_valid() || LATEST_REQUEST.load(Ordering::SeqCst) != instance {
        return;
    }

    std::thread::spawn(move || unsafe {
        let is_valid = || {
            valid.as_ref().is_none_or(|check| check())
                && LATEST_REQUEST.load(Ordering::SeqCst) == instance
        };
        if !is_valid() {
            return;
        }
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
            WS_POPUP,
            screen_x,
            screen_y,
            screen_w,
            screen_h,
            None,
            None,
            Some(hinstance.into()),
            None,
        )
        .and_then(crate::overlay::shell_policy::prepare)
        {
            Ok(value) => value,
            Err(error) => {
                eprintln!("[WindowSelector] CreateWindowExW failed: {error}");
                return;
            }
        };
        let _ = SetWindowLongPtrW(hwnd, GWLP_USERDATA, instance as isize);
        if GetWindowLongPtrW(hwnd, GWLP_USERDATA) != instance as isize {
            let _ = DestroyWindow(hwnd);
            return;
        }

        let margins = MARGINS {
            cxLeftWidth: -1,
            cxRightWidth: -1,
            cyTopHeight: -1,
            cyBottomHeight: -1,
        };
        let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);

        if !is_valid() {
            let _ = DestroyWindow(hwnd);
            return;
        }
        let mut registered = false;
        if !admit(&admission, &mut || {
            let _registration = SELECTOR_REGISTRATION
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if LATEST_REQUEST.load(Ordering::SeqCst) != instance {
                return;
            }
            SELECTOR_HWND.store(hwnd.0 as isize, Ordering::SeqCst);
            SELECTOR_OWNER.store(owner.as_u8(), Ordering::SeqCst);
            SELECTOR_TOKEN.store(token, Ordering::SeqCst);
            SELECTOR_INSTANCE.store(instance, Ordering::SeqCst);
            registered = true;
        }) || !registered
        {
            let _ = DestroyWindow(hwnd);
            return;
        }

        SELECTOR_WEB_CONTEXT.with(|context| {
            if context.borrow().is_none() {
                *context.borrow_mut() = Some(crate::overlay::webview_runtime::create_context(
                    crate::overlay::webview_runtime::Profile::Common,
                ));
            }
        });

        let font_css = crate::overlay::html_components::font_manager::get_font_css();
        let html = generate_html(&entries, &font_css, is_dark, &text);
        let page_url = crate::overlay::html_components::font_manager::store_html_page(html.clone())
            .unwrap_or_else(|| format!("data:text/html,{}", urlencoding::encode(&html)));

        let wrapper = HwndWrapper(hwnd);
        let on_select = callbacks.on_select.clone();
        let on_cancel = callbacks.on_cancel.clone();
        let callback_hwnd = hwnd.0 as isize;
        let webview_result = {
            let init = crate::overlay::webview_init::acquire("window-selector");
            let result = SELECTOR_WEB_CONTEXT.with(|ctx_cell| {
                let mut ctx_ref = ctx_cell.borrow_mut();
                let builder = WebViewBuilder::new_with_web_context(ctx_ref.as_mut().unwrap());
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
                        let _ = PostMessageW(
                            Some(HWND(callback_hwnd as *mut _)),
                            WM_CLOSE,
                            WPARAM(instance as usize),
                            LPARAM(0),
                        );
                    })
                    .build_as_child(&wrapper)
            });
            init.finish(result.is_ok());
            result
        };

        let webview = match webview_result {
            Ok(value) => value,
            Err(error) => {
                eprintln!("[WindowSelector] WebView build failed: {error}");
                let _ = DestroyWindow(hwnd);
                return;
            }
        };
        crate::overlay::webview_diagnostics::attach_webview2_close_on_failure(
            "window-selector",
            hwnd,
            &webview,
        );

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

        if !is_valid() {
            let _ = DestroyWindow(hwnd);
            SELECTOR_WEBVIEW.with(|wv| *wv.borrow_mut() = None);
            SELECTOR_WEB_CONTEXT.with(|ctx| *ctx.borrow_mut() = None);
            return;
        }
        let _ = ShowWindow(hwnd, SW_SHOW);
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
        with_scripts(|queue| queue.discard(hwnd.0 as isize));
    });
}

#[cfg(test)]
#[path = "host_tests.rs"]
mod ownership_tests;
