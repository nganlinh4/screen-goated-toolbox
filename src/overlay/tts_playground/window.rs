//! WRY window for the TTS Playground. Mirrors translation_gummy/window.rs
//! shape: single-instance window, popup chrome with rounded corners, embeds
//! a WebView2 surface that loads the inlined frontend bundle.

use crate::win_types::HwndWrapper;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::{
    DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmExtendFrameIntoClientArea,
    DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Gdi::HBRUSH;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{HSTRING, PCWSTR, w};
use wry::{Rect, WebViewBuilder};

pub(super) fn show() {
    let action = super::WINDOW_LIFECYCLE
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .show();
    match action {
        super::lifecycle::ShowAction::Start => {
            std::thread::spawn(internal_create_loop_entry);
        }
        super::lifecycle::ShowAction::Post(hwnd, generation) => unsafe {
            let _ = PostMessageW(
                Some(HWND(hwnd as *mut std::ffi::c_void)),
                super::WM_APP_SHOW,
                WPARAM(generation),
                LPARAM(0),
            );
        },
        super::lifecycle::ShowAction::Deferred => {}
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
            super::WM_APP_EXIT_DRAIN => {
                super::exit_drain::run_barrier(hwnd, wparam.0, lparam.0 as usize);
                LRESULT(0)
            }
            super::WM_APP_EXIT_THAW => {
                super::exit_drain::run_thaw(hwnd, wparam.0, lparam.0 as usize);
                LRESULT(0)
            }
            super::WM_APP_SHOW => {
                if super::WINDOW_LIFECYCLE
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .ready_window()
                    != Some((hwnd.0 as isize, wparam.0))
                {
                    return LRESULT(0);
                }
                refresh_window_chrome(hwnd);
                super::runtime::hydrate_recent_once();
                super::state::sync_to_webview();
                let _admission = super::REMOVAL_ADMISSION
                    .lock()
                    .unwrap_or_else(|error| error.into_inner());
                if super::WINDOW_LIFECYCLE
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .ready_window()
                    != Some((hwnd.0 as isize, wparam.0))
                {
                    return LRESULT(0);
                }
                let _ = ShowWindow(hwnd, SW_RESTORE);
                let _ = ShowWindow(hwnd, SW_SHOW);
                let _ = SetForegroundWindow(hwnd);
                let _ = SetFocus(Some(hwnd));
                LRESULT(0)
            }
            super::WM_APP_SYNC => {
                if super::WINDOW_LIFECYCLE
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .ready_window()
                    != Some((hwnd.0 as isize, wparam.0))
                {
                    return LRESULT(0);
                }
                // Host theme/language changed — refresh chrome + push the new
                // theme + localized strings into the webview.
                refresh_window_chrome(hwnd);
                super::state::sync_to_webview();
                LRESULT(0)
            }
            super::WM_APP_TICK => {
                if !super::WINDOW_LIFECYCLE
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .is_ready_window(hwnd.0 as isize, wparam.0)
                {
                    return LRESULT(0);
                }
                // Advance the player position and push it so the seek bar/time
                // update live during playback.
                super::runtime::tick_position();
                super::state::sync_to_webview();
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
            WM_CLOSE => {
                if !super::flush_draft_or_notify() {
                    return LRESULT(0);
                }
                let _ = ShowWindow(hwnd, SW_HIDE);
                LRESULT(0)
            }
            super::WM_APP_REMOVE => {
                let request = wparam.0;
                if request == 0
                    || super::ACTIVE_REMOVAL.load(std::sync::atomic::Ordering::Acquire) != request
                {
                    return LRESULT(0);
                }
                if !super::flush_draft_or_notify() {
                    super::FAILED_REMOVAL.store(request, std::sync::atomic::Ordering::Release);
                    return LRESULT(0);
                }
                let _admission = super::REMOVAL_ADMISSION
                    .lock()
                    .unwrap_or_else(|error| error.into_inner());
                if super::ACTIVE_REMOVAL.load(std::sync::atomic::Ordering::Acquire) != request {
                    return LRESULT(0);
                }
                super::runtime::cancel_generation();
                super::runtime::stop();
                super::runtime_sources::stop_mic_recording();
                let _ = DestroyWindow(hwnd);
                LRESULT(0)
            }
            WM_DESTROY => {
                super::WEBVIEW.with(|webview| {
                    *webview.borrow_mut() = None;
                });
                super::ASSET_PACK.with(|slot| *slot.borrow_mut() = None);
                super::WINDOW_LIFECYCLE
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .destroyed(hwnd.0 as isize);
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

unsafe fn internal_create_loop() {
    let instance = unsafe { GetModuleHandleW(None).unwrap() };
    let class_name = w!("TtsPlaygroundWindowClass");

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
    let width = ((screen_w as f64 * 0.50) as i32).clamp(820, 1100);
    let height = ((screen_h as f64 * 0.55) as i32).clamp(540, 720);
    let rect = area.centered(width, height);

    let title = HSTRING::from(
        crate::gui::locale::LocaleText::get(&super::current_ui_language())
            .tts_playground
            .tts_playground_title,
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
        super::WINDOW_LIFECYCLE
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .creation_failed();
        return;
    }

    let window_generation = super::WINDOW_LIFECYCLE
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .created(hwnd.0 as isize);

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

    super::runtime::hydrate_recent_once();
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
            window.__TTS_INITIAL_STATE__ = {initial_payload};
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

    let (inlined_html, web_asset_pack) = match super::assets::build_inlined_html() {
        Ok(assets) => assets,
        Err(error) => {
            crate::log_info!("[TTS Playground] Could not open interface assets: {error}");
            super::WINDOW_LIFECYCLE
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .creation_failed();
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
            return;
        }
    };
    let Some(page_url) =
        crate::overlay::html_components::font_manager::store_html_page(inlined_html)
    else {
        crate::log_info!("[TTS Playground] Could not create the interface page");
        super::WINDOW_LIFECYCLE
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .creation_failed();
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
        return;
    };

    let created = super::WEB_CONTEXT.with(|context| {
        let mut context_ref = context.borrow_mut();
        if context_ref.is_none() {
            *context_ref = Some(crate::overlay::webview_runtime::create_context(
                crate::overlay::webview_runtime::Profile::Common,
            ));
        }

        let webview_result = {
            let init = crate::overlay::webview_init::acquire("tts-playground");
            let expected_page = page_url.clone();
            let window_id = hwnd.0 as isize;
            let builder = WebViewBuilder::new_with_web_context(context_ref.as_mut().unwrap())
                .with_background_color(background)
                .with_initialization_script(&init_script)
                .with_ipc_handler(move |request: wry::http::Request<String>| {
                    super::ipc::handle_ipc(hwnd, request.body());
                })
                .with_navigation_handler(move |url| {
                    super::WINDOW_LIFECYCLE
                        .lock()
                        .unwrap_or_else(|error| error.into_inner())
                        .admit_initial_document(window_id, window_generation, &expected_page, &url)
                })
                .with_url(&page_url);
            let result = builder.build_as_child(&wrapper);
            init.finish(result.is_ok());
            result
        };

        match webview_result {
            Ok(webview) => {
                crate::overlay::webview_diagnostics::attach_webview2_diagnostics(
                    "tts-playground",
                    hwnd,
                    &webview,
                );
                super::WEBVIEW.with(|slot| *slot.borrow_mut() = Some(webview));
                super::ASSET_PACK.with(|slot| *slot.borrow_mut() = Some(web_asset_pack));
                true
            }
            Err(error) => {
                crate::log_info!("[TTS Playground] Could not create the interface: {error}");
                false
            }
        }
    });
    if !created {
        super::WINDOW_LIFECYCLE
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .creation_failed();
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
        return;
    }

    resize_webview(hwnd);

    let generation = unsafe {
        let _ = ShowWindow(hwnd, SW_HIDE);
        let ready = super::WINDOW_LIFECYCLE
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .ready(hwnd.0 as isize);
        let Some((generation, show)) = ready else {
            return;
        };
        if show {
            let _ = PostMessageW(
                Some(hwnd),
                super::WM_APP_SHOW,
                WPARAM(generation),
                LPARAM(0),
            );
        }
        generation
    };
    super::state::sync_to_webview();
    spawn_playback_ticker(hwnd, generation);

    unsafe {
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        if IsWindow(Some(hwnd)).as_bool() {
            let _ = DestroyWindow(hwnd);
        }
        super::WEBVIEW.with(|webview| {
            *webview.borrow_mut() = None;
        });
        super::WEB_CONTEXT.with(|context| {
            *context.borrow_mut() = None;
        });
        super::ASSET_PACK.with(|slot| *slot.borrow_mut() = None);
    }
}

fn internal_create_loop_entry() {
    unsafe {
        internal_create_loop();
    }
    if super::WINDOW_LIFECYCLE
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .thread_finished()
    {
        std::thread::spawn(internal_create_loop_entry);
    }
}

/// Posts `WM_APP_TICK` to the window ~6×/s while audio is playing, so the
/// player UI advances without waiting for a user-driven IPC round-trip. Idle
/// (no post) when nothing is playing, so it costs nothing at rest.
fn spawn_playback_ticker(hwnd: HWND, generation: usize) {
    let target = crate::win_types::SendHwnd(hwnd);
    std::thread::spawn(move || {
        // Capture the whole SendHwnd (Send) rather than the bare HWND field.
        let target = target;
        loop {
            std::thread::sleep(std::time::Duration::from_millis(160));
            let ready = super::WINDOW_LIFECYCLE
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .is_ready_window(target.0.0 as isize, generation);
            if !ready {
                return;
            }
            let playing = super::state::with_state(|s| s.is_playing);
            if playing {
                unsafe {
                    let _ = PostMessageW(
                        Some(target.0),
                        super::WM_APP_TICK,
                        WPARAM(generation),
                        LPARAM(0),
                    );
                }
            }
        }
    });
}

fn refresh_window_chrome(hwnd: HWND) {
    let is_dark = crate::overlay::is_dark_mode();
    crate::gui::utils::set_window_icon(hwnd, is_dark);
    let title = HSTRING::from(
        crate::gui::locale::LocaleText::get(&super::current_ui_language())
            .tts_playground
            .tts_playground_title,
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
