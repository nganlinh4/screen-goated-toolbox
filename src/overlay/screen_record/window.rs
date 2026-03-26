// --- SCREEN RECORD WINDOW CREATION ---
// WebView window setup, custom protocol handler, and IPC bridge initialization.

use std::borrow::Cow;
use std::thread;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::{DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmSetWindowAttribute};
use windows::Win32::Graphics::Gdi::CreateSolidBrush;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;
use wry::{Rect, WebContext, WebViewBuilder};

use crate::assets::GOOGLE_SANS_FLEX;
use crate::win_types::SendHwnd;

use super::{
    HwndWrapper, REGISTER_SR_CLASS, SR_HWND, SR_WEBVIEW, SR_WEB_CONTEXT,
    WM_APP_RUN_SCRIPT, WM_APP_SHOW,
    embedded_assets, gpu_export, ipc, native_export,
    sr_wnd_proc, try_read_downloaded_bg, try_read_runtime_cursor_svg, wnd_http_response,
    IpcRequest, SERVER_PORT,
};

pub(super) fn push_settings_to_webview() {
    let (lang, theme_mode) = {
        let app = crate::APP.lock().unwrap();
        (
            app.config.ui_language.clone(),
            app.config.theme_mode.clone(),
        )
    };

    let theme_str = match theme_mode {
        crate::config::ThemeMode::Dark => "dark",
        crate::config::ThemeMode::Light => "light",
        crate::config::ThemeMode::System => {
            if crate::gui::utils::is_system_in_dark_mode() {
                "dark"
            } else {
                "light"
            }
        }
    };

    // Update window icon based on theme
    unsafe {
        let hwnd = std::ptr::addr_of!(SR_HWND).read();
        if !hwnd.is_invalid() {
            crate::gui::utils::set_window_icon(hwnd.0, theme_str == "dark");
        }
    }

    SR_WEBVIEW.with(|wv| {
        if let Some(webview) = wv.borrow().as_ref() {
            let script = format!(
                "window.postMessage({{ type: 'sr-set-settings', theme: '{}', lang: '{}' }}, '*');",
                theme_str, lang
            );
            let _ = webview.evaluate_script(&script);
        }
    });
}

// --- WINDOW CREATION ---

pub(super) unsafe fn internal_create_sr_loop() {
    unsafe {
        let instance = GetModuleHandleW(None).unwrap();
        let class_name = windows::core::w!("ScreenRecord_Class");

        REGISTER_SR_CLASS.call_once(|| {
            let wc = WNDCLASSW {
                lpfnWndProc: Some(sr_wnd_proc),
                hInstance: instance.into(),
                lpszClassName: class_name,
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap(),
                hbrBackground: CreateSolidBrush(COLORREF(0x00111111)),
                ..Default::default()
            };
            let _ = RegisterClassW(&wc);
        });

        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let screen_h = GetSystemMetrics(SM_CYSCREEN);

        let (width, height) = {
            let app = crate::APP.lock().unwrap();
            let (w, h) = app.config.screen_record_window_size;
            (w.max(1000), h.max(500))
        };
        let x = (screen_w - width) / 2;
        let y = (screen_h - height) / 2;

        let hwnd = CreateWindowExW(
            WS_EX_APPWINDOW,
            class_name,
            windows::core::w!("SGT Record"),
            WS_POPUP
                | WS_THICKFRAME
                | WS_CAPTION
                | WS_SYSMENU
                | WS_MINIMIZEBOX
                | WS_MAXIMIZEBOX
                | WS_CLIPCHILDREN,
            x,
            y,
            width,
            height,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .unwrap();

        SR_HWND = SendHwnd(hwnd);

        let corner_pref = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &corner_pref as *const _ as *const std::ffi::c_void,
            std::mem::size_of_val(&corner_pref) as u32,
        );

        let wrapper = HwndWrapper(hwnd);

        SR_WEB_CONTEXT.with(|ctx| {
            if ctx.borrow().is_none() {
                let shared_data_dir = crate::overlay::get_shared_webview_data_dir(Some("common"));
                *ctx.borrow_mut() = Some(WebContext::new(Some(shared_data_dir)));
            }
        });

        std::thread::sleep(std::time::Duration::from_millis(100));

        let font_style_tag = r#"<style id="sgt-font-face">
        @font-face {
            font-family: 'Google Sans Flex';
            src: url('/font.ttf') format('truetype');
            font-weight: 100 1000;
            font-style: normal;
            font-display: swap;
        }
    </style>"#
            .to_string();

        // Read initial theme/lang from config
        let (init_lang, init_theme_mode) = {
            let app = crate::APP.lock().unwrap();
            (
                app.config.ui_language.clone(),
                app.config.theme_mode.clone(),
            )
        };
        let init_theme = match init_theme_mode {
            crate::config::ThemeMode::Dark => "dark",
            crate::config::ThemeMode::Light => "light",
            crate::config::ThemeMode::System => {
                if crate::gui::utils::is_system_in_dark_mode() {
                    "dark"
                } else {
                    "light"
                }
            }
        };
        let webview_background_rgba = if init_theme == "dark" {
            (9, 9, 11, 255)
        } else {
            (250, 250, 250, 255)
        };
        let themed_html_root = if init_theme == "dark" {
            "<html lang=\"en\" class=\"dark\" data-sr-initial-theme=\"dark\">"
        } else {
            "<html lang=\"en\" data-sr-initial-theme=\"light\">"
        };

        // Set window icon based on initial theme
        crate::gui::utils::set_window_icon(hwnd, init_theme == "dark");

        let init_script = format!(
            r#"
        (function() {{
            const originalPostMessage = window.ipc.postMessage;
            window.isWry = true;
            window.invoke = async (cmd, args = {{}}) => {{
                return new Promise((resolve, reject) => {{
                    const id = Math.random().toString(36).substring(7);
                    const handler = (e) => {{
                        if (e.detail && e.detail.id === id) {{
                            window.removeEventListener('ipc-reply', handler);
                            if (e.detail.error) reject(e.detail.error);
                            else resolve(e.detail.result);
                        }}
                    }};
                    window.addEventListener('ipc-reply', handler);
                    originalPostMessage(JSON.stringify({{ id, cmd, args }}));
                }});
            }};
            // Set initial settings synchronously so React can read on mount
            window.__SR_INITIAL_THEME__ = '{init_theme}';
            window.__SR_INITIAL_LANG__ = '{init_lang}';
            document.title = 'SGT Record';
            if (document.documentElement) {{
                if ('{init_theme}' === 'dark') {{
                    document.documentElement.classList.add('dark');
                }} else {{
                    document.documentElement.classList.remove('dark');
                }}
            }}
        }})();
    "#
        );

        let webview_result = {
            let _init_lock = crate::overlay::GLOBAL_WEBVIEW_MUTEX.lock().unwrap();

            SR_WEB_CONTEXT.with(|ctx| {
            let mut ctx_ref = ctx.borrow_mut();
            let mut builder = WebViewBuilder::new_with_web_context(ctx_ref.as_mut().unwrap())
                .with_background_color(webview_background_rgba)
                .with_custom_protocol("screenrecord".to_string(), {
                    let font_style_tag = font_style_tag.clone();
                    let themed_html_root = themed_html_root.to_string();
                    move |_id, request| {
                    let path = request.uri().path();
                    if path == "/font.ttf" {
                        return wnd_http_response(200, "font/ttf", Cow::Borrowed(GOOGLE_SANS_FLEX));
                    }
                    if let Some(bytes) = try_read_runtime_cursor_svg(path) {
                        return wnd_http_response(200, "image/svg+xml", Cow::Owned(bytes));
                    }
                    if let Some((bytes, mime)) = try_read_downloaded_bg(path) {
                        return wnd_http_response(200, mime, Cow::Owned(bytes));
                    }
                    let (content, mime) = if path == "/" || path == "/index.html" {
                        // Inject initial theme class and font CSS into HTML <head> before React mounts.
                        let html = String::from_utf8_lossy(embedded_assets::INDEX_HTML);
                        let themed = html.replace("<html lang=\"en\">", &themed_html_root);
                        let modified = themed.replace("</head>", &format!("{font_style_tag}</head>"));
                        (Cow::Owned(modified.into_bytes()), "text/html")
                    } else if let Some((bytes, mime)) = embedded_assets::lookup_packaged_asset(path) {
                        (Cow::Borrowed(bytes), mime)
                    } else {
                        return wnd_http_response(
                            404,
                            "text/plain",
                            Cow::Borrowed(b"Not Found".as_slice()),
                        );
                    };
                    wnd_http_response(200, mime, content)
                }})
                .with_initialization_script(&init_script)
                .with_ipc_handler(build_ipc_handler(hwnd))
                .with_url("screenrecord://localhost/index.html");

            builder = crate::overlay::html_components::font_manager::configure_webview(builder);
            builder.build_as_child(&wrapper)
        })
        };

        let webview = match webview_result {
            Ok(wv) => wv,
            Err(e) => {
                eprintln!("Failed to create ScreenRecord WebView: {:?}", e);
                let _ = DestroyWindow(hwnd);
                SR_HWND = SendHwnd::default();
                return;
            }
        };
        let mut r = RECT::default();
        let _ = GetClientRect(hwnd, &mut r);
        let _ = webview.set_bounds(Rect {
            position: wry::dpi::Position::Physical(wry::dpi::PhysicalPosition::new(0, 0)),
            size: wry::dpi::Size::Physical(wry::dpi::PhysicalSize::new(
                (r.right - r.left).max(0) as u32,
                (r.bottom - r.top).max(0) as u32,
            )),
        });

        SR_WEBVIEW.with(|wv| {
            *wv.borrow_mut() = Some(webview);
        });

        super::IS_WARMED_UP = true;

        let port = ipc::start_global_media_server().unwrap_or(0);
        SERVER_PORT.store(port, std::sync::atomic::Ordering::SeqCst);

        // Eagerly initialize the shared GPU context (wgpu device + pipelines) in
        // the background. This takes ~8s on first run and is cached forever via
        // OnceLock, so doing it early avoids blocking the first export.
        thread::spawn(|| {
            gpu_export::eager_init_gpu_context();
        });

        // Prepare export GPU pipeline in the background once the recorder has been
        // idle long enough. Warm-up is useful for first export, but running it
        // during active capture steals GPU time from recording.
        thread::spawn(|| {
            native_export::warm_up_export_pipeline_when_idle();
        });

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            let _ = DispatchMessageW(&msg);
        }

        SR_WEBVIEW.with(|wv| {
            *wv.borrow_mut() = None;
        });
        SR_HWND = SendHwnd::default();
        super::IS_WARMED_UP = false;
        super::IS_INITIALIZING = false;
    }
}

/// Builds the IPC handler closure for the WebView
fn build_ipc_handler(hwnd: HWND) -> impl Fn(wry::http::Request<String>) + 'static {
    use super::PRE_FULLSCREEN_RECT;
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
    };
    use windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;

    let send_hwnd = SendHwnd(hwnd);
    move |msg: wry::http::Request<String>| {
        let body = msg.body().as_str();
        let hwnd = send_hwnd.0;
        unsafe {
            if body == "drag_window" {
                let _ = ReleaseCapture();
                let _ = SendMessageW(
                    hwnd,
                    WM_NCLBUTTONDOWN,
                    Some(WPARAM(HTCAPTION as usize)),
                    Some(LPARAM(0)),
                );
            } else if let Some(dir) = body.strip_prefix("resize_") {
                let ht = match dir {
                    "n" => HTTOP as usize,
                    "s" => HTBOTTOM as usize,
                    "w" => HTLEFT as usize,
                    "e" => HTRIGHT as usize,
                    "nw" => HTTOPLEFT as usize,
                    "ne" => HTTOPRIGHT as usize,
                    "sw" => HTBOTTOMLEFT as usize,
                    "se" => HTBOTTOMRIGHT as usize,
                    _ => 0,
                };
                if ht != 0 {
                    let _ = ReleaseCapture();
                    let _ = SendMessageW(
                        hwnd,
                        WM_NCLBUTTONDOWN,
                        Some(WPARAM(ht)),
                        Some(LPARAM(0)),
                    );
                }
            } else if body == "minimize_window" {
                let _ = ShowWindow(hwnd, SW_MINIMIZE);
            } else if body == "toggle_maximize" {
                if IsZoomed(hwnd).as_bool() {
                    let _ = ShowWindow(hwnd, SW_RESTORE);
                } else {
                    let _ = ShowWindow(hwnd, SW_MAXIMIZE);
                }
            } else if body == "close_window" {
                let _ = ShowWindow(hwnd, SW_HIDE);
            } else if body == "enter_fullscreen" {
                let mut rect = RECT::default();
                let _ = GetWindowRect(hwnd, &mut rect);
                *PRE_FULLSCREEN_RECT.lock().unwrap() = Some((
                    rect.left, rect.top, rect.right, rect.bottom,
                ));
                let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
                let mut mi = MONITORINFO {
                    cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                    ..Default::default()
                };
                let _ = GetMonitorInfoW(monitor, &mut mi);
                let r = mi.rcMonitor;
                let _ = SetWindowPos(
                    hwnd,
                    Some(HWND_TOPMOST),
                    r.left, r.top,
                    r.right - r.left, r.bottom - r.top,
                    SWP_NOACTIVATE | SWP_SHOWWINDOW,
                );
            } else if body == "exit_fullscreen" {
                let saved = PRE_FULLSCREEN_RECT.lock().unwrap().take();
                if let Some((l, t, r, b)) = saved {
                    let _ = SetWindowPos(
                        hwnd,
                        Some(HWND_NOTOPMOST),
                        l, t, r - l, b - t,
                        SWP_NOACTIVATE | SWP_SHOWWINDOW,
                    );
                } else {
                    let _ = SetWindowPos(
                        hwnd,
                        Some(HWND_NOTOPMOST),
                        0, 0, 0, 0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                    );
                }
            } else if let Ok(req) = {
                let t0 = std::time::Instant::now();
                let r = serde_json::from_str::<IpcRequest>(body);
                let elapsed = t0.elapsed();
                if elapsed.as_millis() > 50 {
                    eprintln!("[IPC] Body parse: {:.0}ms ({}KB)", elapsed.as_secs_f64() * 1000.0, body.len() / 1024);
                }
                r
            } {
                let id = req.id;
                let cmd = req.cmd;
                let args = req.args;
                let target_hwnd_val = send_hwnd.as_isize();

                thread::spawn(move || {
                    let result = ipc::handle_ipc_command(cmd, args);
                    let json_res = match result {
                        Ok(res) => serde_json::json!({ "id": id, "result": res }),
                        Err(err) => serde_json::json!({ "id": id, "error": err }),
                    };
                    let script = format!(
                        "window.dispatchEvent(new CustomEvent('ipc-reply', {{ detail: {} }}))",
                        json_res
                    );

                    let script_ptr = Box::into_raw(Box::new(script));
                    let _ = PostMessageW(
                        Some(HWND(target_hwnd_val as *mut std::ffi::c_void)),
                        WM_APP_RUN_SCRIPT,
                        WPARAM(0),
                        LPARAM(script_ptr as isize),
                    );
                });
            }
        }
    }
}
