use raw_window_handle::{
    HandleError, HasWindowHandle, RawWindowHandle, Win32WindowHandle, WindowHandle,
};
use std::borrow::Cow;
use std::num::NonZeroIsize;
use std::sync::Arc;
use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
};
use windows::Win32::Graphics::Gdi::{GetStockObject, BLACK_BRUSH, HBRUSH};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
use windows::Win32::UI::WindowsAndMessaging::*;
use wry::{Rect, WebViewBuilder};

// Assets
const INDEX_HTML: &[u8] = include_bytes!("dist/index.html");
const ASSET_INDEX_JS: &[u8] = include_bytes!("dist/assets/index.js");
const ASSET_INDEX_CSS: &[u8] = include_bytes!("dist/assets/index.css");
const ASSET_CUBIC_JS: &[u8] = include_bytes!("dist/assets/cubic.js");
const ASSET_MORPH_JS: &[u8] = include_bytes!("dist/assets/morph-fixed.js");
const ASSET_ROUNDED_JS: &[u8] = include_bytes!("dist/assets/roundedPolygon.js");
const ASSET_UTILS_JS: &[u8] = include_bytes!("dist/assets/utils.js");

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        WM_ERASEBKGND => {
            // Prevent default background painting to avoid flickering/stripes on resize
            LRESULT(1)
        }
        WM_NCCALCSIZE => {
            // Remove the standard non-client area (title bar, borders)
            // This fixes the "white padding" at the top while keeping WS_THICKFRAME for resizing
            if wparam.0 != 0 {
                LRESULT(0)
            } else {
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

// Wrapper for HWND
struct HwndWrapper(HWND);

impl HasWindowHandle for HwndWrapper {
    fn window_handle(&self) -> std::result::Result<WindowHandle<'_>, HandleError> {
        let hwnd = self.0 .0 as isize;
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

fn wnd_http_response(
    status: u16,
    content_type: &str,
    body: Cow<'static, [u8]>,
) -> wry::http::Response<Cow<'static, [u8]>> {
    wry::http::Response::builder()
        .status(status)
        .header("Content-Type", content_type)
        .header("Access-Control-Allow-Origin", "*")
        .body(body)
        .unwrap_or_else(|_| {
            wry::http::Response::builder()
                .status(500)
                .body(Cow::Borrowed(b"Internal Error".as_slice()))
                .unwrap()
        })
}

pub fn show_prompt_dj() {
    std::thread::spawn(move || {
        unsafe {
            // 1. Create Window
            let instance = GetModuleHandleW(None).unwrap();
            let class_name = w!("PromptDJ_Class");

            let mut wc = WNDCLASSW::default();
            wc.lpfnWndProc = Some(wnd_proc);
            wc.hInstance = instance.into();
            wc.lpszClassName = class_name;
            wc.hCursor = LoadCursorW(None, IDC_ARROW).unwrap();
            wc.hbrBackground = HBRUSH(std::ptr::null_mut()); // Transparent background
            let _ = RegisterClassW(&wc);

            let width = 1200;
            let height = 800;

            // Center on screen
            let screen_w = GetSystemMetrics(SM_CXSCREEN);
            let screen_h = GetSystemMetrics(SM_CYSCREEN);
            let x = (screen_w - width) / 2;
            let y = (screen_h - height) / 2;

            let (api_key, lang, theme_mode) = {
                let app = crate::APP.lock().unwrap();
                (
                    app.config.gemini_api_key.clone(),
                    app.config.ui_language.clone(),
                    app.config.theme_mode.clone(),
                )
            };

            let title_str = crate::gui::locale::LocaleText::get(&lang).prompt_dj_title;
            let title_wide = windows::core::HSTRING::from(title_str);

            // Use POPUP + THICKFRAME for resizable frameless window
            // WS_EX_TOOLWINDOW hides it from taskbar
            let hwnd = CreateWindowExW(
                WS_EX_TOPMOST,
                class_name,
                PCWSTR(title_wide.as_ptr()),
                WS_POPUP | WS_VISIBLE | WS_THICKFRAME,
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

            // Enable rounded corners
            let corner_pref = DWMWCP_ROUND;
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_WINDOW_CORNER_PREFERENCE,
                &corner_pref as *const _ as *const std::ffi::c_void,
                std::mem::size_of_val(&corner_pref) as u32,
            );

            // 2. Create WebView
            let wrapper = HwndWrapper(hwnd);

            let theme_str = match theme_mode {
                crate::config::ThemeMode::Dark => "dark",
                crate::config::ThemeMode::Light => "light",
                crate::config::ThemeMode::System => {
                    // Simple heuristic or default to dark if not easily queryable without winit
                    "dark"
                }
            };

            // Get local font CSS (cached fonts, no network loading)
            let font_css = crate::overlay::html_components::font_manager::get_font_css();

            // Init script: Injects API key AND adds a custom title bar for dragging
            let init_script = format!(
                r#"
                window.addEventListener('load', () => {{
                    // Inject styling for draggable header and body reset
                    const style = document.createElement('style');
                    style.innerHTML = `{}` + `
                        body {{
                            margin: 0;
                            padding: 0;
                            font-family: 'Google Sans Flex', 'Segoe UI', system-ui, sans-serif !important;
                            background-color: transparent !important;
                            overflow: hidden;
                        }}
                        #dj-drag-header {{
                            position: fixed;
                            top: 0;
                            left: 0;
                            width: 100%;
                            height: 32px;
                            background: transparent;
                            z-index: 9999;
                            -webkit-app-region: drag; 
                            cursor: grab;
                        }}
                        #dj-drag-header:active {{
                            cursor: grabbing;
                        }}
                        #dj-close-btn {{
                            position: absolute;
                            top: 0;
                            right: 0;
                            width: 40px;
                            height: 32px;
                            background: transparent;
                            color: rgba(255,255,255,0.5);
                            border: none;
                            font-family: 'Google Sans Flex', 'Segoe UI', system-ui;
                            font-size: 16px;
                            cursor: pointer;
                            display: flex;
                            align-items: center;
                            justify-content: center;
                            transition: background 0.2s, color 0.2s;
                        }}
                        #dj-close-btn:hover {{
                            background: rgba(255,0,0,0.5);
                            color: white;
                        }}
                        #dj-min-btn {{
                            position: absolute;
                            top: 0;
                            right: 40px;
                            width: 40px;
                            height: 32px;
                            background: transparent;
                            color: rgba(255,255,255,0.5);
                            border: none;
                            font-family: 'Google Sans Flex', 'Segoe UI', system-ui;
                            font-size: 16px;
                            cursor: pointer;
                            display: flex;
                            align-items: center;
                            justify-content: center;
                            transition: background 0.2s, color 0.2s;
                        }}
                        #dj-min-btn:hover {{
                            background: rgba(255,255,255,0.1);
                            color: white;
                        }}
                    `;
                    document.head.appendChild(style);

                    // Create draggable header
                    const header = document.createElement('div');
                    header.id = 'dj-drag-header';
                    
                    const minBtn = document.createElement('button');
                    minBtn.id = 'dj-min-btn';
                    minBtn.innerHTML = '—';
                    minBtn.onclick = (e) => {{
                        e.stopPropagation(); 
                        if (window.ipc) window.ipc.postMessage('minimize_window');
                    }};
                    header.appendChild(minBtn);

                    const closeBtn = document.createElement('button');
                    closeBtn.id = 'dj-close-btn';
                    closeBtn.innerHTML = '✕';
                    closeBtn.onclick = (e) => {{
                        e.stopPropagation(); 
                        if (window.ipc) window.ipc.postMessage('close_window');
                    }};
                    header.appendChild(closeBtn);

                    header.addEventListener('mousedown', (e) => {{
                        if (e.target !== closeBtn && e.target !== minBtn) {{
                            if (window.ipc) window.ipc.postMessage('drag_window');
                        }}
                    }});

                    document.body.appendChild(header);

                    setTimeout(() => {{
                        window.postMessage({{ type: 'pm-dj-set-api-key', apiKey: '{}', lang: '{}' }}, '*');
                        window.postMessage({{ type: 'pm-dj-set-theme', theme: '{}' }}, '*');
                        window.postMessage({{ type: 'pm-dj-set-font', font: 'google-sans-flex' }}, '*');
                    }}, 250);
                }});
                "#,
                font_css, api_key, lang, theme_str
            );

            // Capture HWND for move closure
            let hwnd_ipc = hwnd;

            let builder = WebViewBuilder::new()
                .with_custom_protocol("promptdj".to_string(), move |_id, request| {
                    let path = request.uri().path();
                    let (content, mime) = if path == "/" || path == "/index.html" {
                        (Cow::Borrowed(INDEX_HTML), "text/html")
                    } else if path.ends_with("index.js") {
                        (Cow::Borrowed(ASSET_INDEX_JS), "application/javascript")
                    } else if path.ends_with("index.css") {
                        (Cow::Borrowed(ASSET_INDEX_CSS), "text/css")
                    } else if path.ends_with("cubic.js") {
                        (Cow::Borrowed(ASSET_CUBIC_JS), "application/javascript")
                    } else if path.ends_with("morph-fixed.js") {
                        (Cow::Borrowed(ASSET_MORPH_JS), "application/javascript")
                    } else if path.ends_with("roundedPolygon.js") {
                        (Cow::Borrowed(ASSET_ROUNDED_JS), "application/javascript")
                    } else if path.ends_with("utils.js") {
                        (Cow::Borrowed(ASSET_UTILS_JS), "application/javascript")
                    } else {
                        return wnd_http_response(
                            404,
                            "text/plain",
                            Cow::Borrowed(b"Not Found".as_slice()),
                        );
                    };

                    wnd_http_response(200, mime, content)
                })
                .with_initialization_script(&init_script)
                .with_ipc_handler(move |msg: wry::http::Request<String>| {
                    let body = msg.body().as_str();
                    if body == "drag_window" {
                        let _ = ReleaseCapture();
                        SendMessageW(
                            hwnd_ipc,
                            WM_NCLBUTTONDOWN,
                            Some(WPARAM(HTCAPTION as usize)),
                            Some(LPARAM(0)),
                        );
                    } else if body == "minimize_window" {
                        let _ = ShowWindow(hwnd_ipc, SW_MINIMIZE);
                    } else if body == "close_window" {
                        let _ = DestroyWindow(hwnd_ipc);
                        PostQuitMessage(0);
                    }
                })
                .with_url("promptdj://localhost/index.html");

            let webview = builder
                .build_as_child(&wrapper)
                .expect("Failed to create PromptDJ WebView");
            let webview_arc = Arc::new(webview);

            // Initial Resize
            unsafe {
                let mut r = RECT::default();
                let _ = GetClientRect(hwnd, &mut r);
                let width = r.right - r.left;
                let height = r.bottom - r.top;
                let _ = webview_arc.set_bounds(Rect {
                    position: wry::dpi::Position::Physical(wry::dpi::PhysicalPosition::new(0, 0)),
                    size: wry::dpi::Size::Physical(wry::dpi::PhysicalSize::new(
                        width as u32,
                        height as u32,
                    )),
                });
            }

            // 3. Message Loop
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).into() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);

                if msg.message == WM_SIZE {
                    let mut r = RECT::default();
                    let _ = GetClientRect(hwnd, &mut r);
                    let width = r.right - r.left;
                    let height = r.bottom - r.top;
                    // Resize webview
                    let _ = webview_arc.set_bounds(Rect {
                        position: wry::dpi::Position::Physical(wry::dpi::PhysicalPosition::new(
                            0, 0,
                        )),
                        size: wry::dpi::Size::Physical(wry::dpi::PhysicalSize::new(
                            width as u32,
                            height as u32,
                        )),
                    });
                }
            }
        }
    });
}
