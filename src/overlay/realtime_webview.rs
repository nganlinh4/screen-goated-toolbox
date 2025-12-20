//! WebView2-based realtime transcription overlay
//! 
//! Uses smooth scrolling for a non-eye-sore reading experience.
//! Text appends at the bottom, viewport smoothly slides up.

use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::*;
use windows::core::*;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}, Mutex, Once};
use std::num::NonZeroIsize;
use std::collections::HashMap;
use wry::{WebViewBuilder, Rect};
use raw_window_handle::{HasWindowHandle, RawWindowHandle, WindowHandle, Win32WindowHandle, HandleError};
use crate::APP;
use crate::api::realtime_audio::{
    start_realtime_transcription, RealtimeState, SharedRealtimeState,
    WM_REALTIME_UPDATE, WM_TRANSLATION_UPDATE,
};

// Window dimensions
const OVERLAY_WIDTH: i32 = 500;
const OVERLAY_HEIGHT: i32 = 180;
const TRANSLATION_WIDTH: i32 = 500;
const TRANSLATION_HEIGHT: i32 = 180;
const GAP: i32 = 20;

lazy_static::lazy_static! {
    pub static ref REALTIME_STOP_SIGNAL: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
    static ref REALTIME_STATE: SharedRealtimeState = Arc::new(Mutex::new(RealtimeState::new()));
}

static mut REALTIME_HWND: HWND = HWND(0);
static mut TRANSLATION_HWND: HWND = HWND(0);
static mut IS_ACTIVE: bool = false;

static REGISTER_REALTIME_CLASS: Once = Once::new();
static REGISTER_TRANSLATION_CLASS: Once = Once::new();

// Thread-local storage for WebViews
thread_local! {
    static REALTIME_WEBVIEWS: std::cell::RefCell<HashMap<isize, wry::WebView>> = std::cell::RefCell::new(HashMap::new());
}

/// Wrapper for HWND to implement HasWindowHandle
struct HwndWrapper(HWND);

impl HasWindowHandle for HwndWrapper {
    fn window_handle(&self) -> std::result::Result<WindowHandle<'_>, HandleError> {
        let hwnd = self.0.0 as isize;
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

/// CSS and HTML for the realtime overlay with smooth scrolling
fn get_realtime_html(is_translation: bool) -> String {
    let title = if is_translation { "🌐 Bản dịch" } else { "🎤 Đang nghe..." };
    let glow_color = if is_translation { "#ff9633" } else { "#00c8ff" };
    
    format!(r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <style>
        * {{ margin: 0; padding: 0; box-sizing: border-box; }}
        html, body {{
            height: 100%;
            overflow: hidden;
            background: rgba(26, 26, 26, 0.95);
            font-family: 'Segoe UI', sans-serif;
            color: #fff;
            border-radius: 12px;
            border: 1px solid {glow_color}40;
            box-shadow: 0 0 20px {glow_color}30;
        }}
        #container {{
            display: flex;
            flex-direction: column;
            height: 100%;
            padding: 10px 15px;
            cursor: grab;
        }}
        #container:active {{
            cursor: grabbing;
        }}
        #header {{
            display: flex;
            justify-content: space-between;
            align-items: center;
            margin-bottom: 8px;
            flex-shrink: 0;
        }}
        #title {{
            font-size: 13px;
            font-weight: bold;
            color: #aaa;
        }}
        #close-btn {{
            font-size: 16px;
            color: #666;
            cursor: pointer;
            padding: 2px 6px;
            border-radius: 4px;
            transition: all 0.2s;
        }}
        #close-btn:hover {{
            color: #fff;
            background: rgba(255,255,255,0.1);
        }}
        #viewport {{
            flex: 1;
            overflow: hidden;
            position: relative;
        }}
        #content {{
            font-size: 16px;
            line-height: 1.5;
            padding-bottom: 5px;
        }}
        .placeholder {{
            color: #666;
            font-style: italic;
        }}
    </style>
</head>
<body>
    <div id="container">
        <div id="header">
            <div id="title">{title}</div>
            <span id="close-btn">✕</span>
        </div>
        <div id="viewport">
            <div id="content">
                <span class="placeholder">Chờ giọng nói...</span>
            </div>
        </div>
    </div>
    <script>
        const container = document.getElementById('container');
        const viewport = document.getElementById('viewport');
        const content = document.getElementById('content');
        const closeBtn = document.getElementById('close-btn');
        
        // Drag support - call IPC to start native window drag
        container.addEventListener('mousedown', function(e) {{
            // Don't start drag if clicking close button
            if (e.target.id === 'close-btn') return;
            window.ipc.postMessage('startDrag');
        }});
        
        // Close button
        closeBtn.addEventListener('click', function(e) {{
            e.stopPropagation();
            window.ipc.postMessage('close');
        }});
        
        let isFirstText = true;
        let currentScrollTop = 0;
        let targetScrollTop = 0;
        let animationFrame = null;
        let lastContentHeight = 0;
        
        // Smooth scroll animation - very gentle, no jumps
        function animateScroll() {{
            const diff = targetScrollTop - currentScrollTop;
            
            if (Math.abs(diff) > 0.5) {{
                // Very slow easing for smooth experience
                const ease = Math.min(0.08, Math.max(0.02, Math.abs(diff) / 1000));
                currentScrollTop += diff * ease;
                viewport.scrollTop = currentScrollTop;
                animationFrame = requestAnimationFrame(animateScroll);
            }} else {{
                currentScrollTop = targetScrollTop;
                viewport.scrollTop = currentScrollTop;
                animationFrame = null;
            }}
        }}
        
        function updateText(text) {{
            if (isFirstText && text) {{
                content.innerHTML = '';
                isFirstText = false;
            }}
            
            if (!text) {{
                content.innerHTML = '<span class="placeholder">Chờ giọng nói...</span>';
                isFirstText = true;
                targetScrollTop = 0;
                if (!animationFrame) {{
                    animationFrame = requestAnimationFrame(animateScroll);
                }}
                return;
            }}
            
            // Update text content
            content.textContent = text;
            
            // Calculate scroll needed
            const contentHeight = content.offsetHeight;
            const viewportHeight = viewport.offsetHeight;
            
            // Only scroll if content exceeds viewport
            if (contentHeight > viewportHeight) {{
                // How much we need to scroll to see the bottom
                const maxScroll = contentHeight - viewportHeight;
                
                // If content just grew (new line), smoothly adjust
                if (contentHeight > lastContentHeight) {{
                    // New content added - scroll to show it
                    targetScrollTop = maxScroll;
                }}
                
                // Ensure we don't jump backwards
                if (targetScrollTop < currentScrollTop - 50) {{
                    // Only allow small backward adjustments
                    targetScrollTop = currentScrollTop - 50;
                }}
                
                // Clamp to valid range
                targetScrollTop = Math.max(0, Math.min(maxScroll, targetScrollTop));
            }} else {{
                // Content fits - stay at top
                targetScrollTop = 0;
            }}
            
            lastContentHeight = contentHeight;
            
            // Start animation if not running
            if (!animationFrame) {{
                animationFrame = requestAnimationFrame(animateScroll);
            }}
        }}
        
        // Expose to Rust via IPC
        window.updateText = updateText;
    </script>
</body>
</html>"#)
}

pub fn is_realtime_overlay_active() -> bool {
    unsafe { IS_ACTIVE && REALTIME_HWND.0 != 0 }
}

pub fn show_realtime_overlay(preset_idx: usize) {
    unsafe {
        if IS_ACTIVE { return; }
        
        let preset = APP.lock().unwrap().config.presets[preset_idx].clone();
        
        // Reset state
        IS_ACTIVE = true;
        REALTIME_STOP_SIGNAL.store(false, Ordering::SeqCst);
        {
            let mut state = REALTIME_STATE.lock().unwrap();
            *state = RealtimeState::new();
        }
        
        let instance = GetModuleHandleW(None).unwrap();
        
        // --- Create Main Realtime Overlay ---
        let class_name = w!("RealtimeWebViewOverlay");
        REGISTER_REALTIME_CLASS.call_once(|| {
            let mut wc = WNDCLASSW::default();
            wc.lpfnWndProc = Some(realtime_wnd_proc);
            wc.hInstance = instance;
            wc.hCursor = LoadCursorW(None, IDC_ARROW).unwrap();
            wc.lpszClassName = class_name;
            wc.style = CS_HREDRAW | CS_VREDRAW;
            wc.hbrBackground = HBRUSH(0);
            let _ = RegisterClassW(&wc);
        });
        
        // Calculate positions
        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let screen_h = GetSystemMetrics(SM_CYSCREEN);
        
        let has_translation = preset.blocks.len() > 1;
        
        let (main_x, main_y) = if has_translation {
            let total_w = OVERLAY_WIDTH * 2 + GAP;
            ((screen_w - total_w) / 2, (screen_h - OVERLAY_HEIGHT) / 2)
        } else {
            ((screen_w - OVERLAY_WIDTH) / 2, (screen_h - OVERLAY_HEIGHT) / 2)
        };
        
        // Create window with WS_EX_LAYERED for transparency support
        let main_hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            class_name,
            w!("Realtime Transcription"),
            WS_POPUP | WS_VISIBLE,
            main_x, main_y, OVERLAY_WIDTH, OVERLAY_HEIGHT,
            None, None, instance, None
        );
        
        REALTIME_HWND = main_hwnd;
        
        // Create WebView for main overlay
        create_realtime_webview(main_hwnd, false);
        
        // --- Create Translation Overlay if needed ---
        let translation_hwnd = if has_translation {
            let trans_class = w!("RealtimeTranslationWebViewOverlay");
            REGISTER_TRANSLATION_CLASS.call_once(|| {
                let mut wc = WNDCLASSW::default();
                wc.lpfnWndProc = Some(translation_wnd_proc);
                wc.hInstance = instance;
                wc.hCursor = LoadCursorW(None, IDC_ARROW).unwrap();
                wc.lpszClassName = trans_class;
                wc.style = CS_HREDRAW | CS_VREDRAW;
                wc.hbrBackground = HBRUSH(0);
                let _ = RegisterClassW(&wc);
            });
            
            let trans_x = main_x + OVERLAY_WIDTH + GAP;
            let trans_hwnd = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                trans_class,
                w!("Translation"),
                WS_POPUP | WS_VISIBLE,
                trans_x, main_y, TRANSLATION_WIDTH, TRANSLATION_HEIGHT,
                None, None, instance, None
            );
            
            TRANSLATION_HWND = trans_hwnd;
            create_realtime_webview(trans_hwnd, true);
            
            Some(trans_hwnd)
        } else {
            TRANSLATION_HWND = HWND(0);
            None
        };
        
        // Start realtime transcription
        start_realtime_transcription(
            preset,
            REALTIME_STOP_SIGNAL.clone(),
            main_hwnd,
            translation_hwnd,
            REALTIME_STATE.clone(),
        );
        
        // Message loop
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
            if msg.message == WM_QUIT { break; }
        }
        
        // Cleanup
        destroy_realtime_webview(REALTIME_HWND);
        if TRANSLATION_HWND.0 != 0 {
            destroy_realtime_webview(TRANSLATION_HWND);
        }
        
        IS_ACTIVE = false;
        REALTIME_HWND = HWND(0);
        TRANSLATION_HWND = HWND(0);
    }
}

fn create_realtime_webview(hwnd: HWND, is_translation: bool) {
    let hwnd_key = hwnd.0 as isize;
    
    let mut rect = RECT::default();
    unsafe { GetClientRect(hwnd, &mut rect); }
    
    let html = get_realtime_html(is_translation);
    let wrapper = HwndWrapper(hwnd);
    
    // Capture hwnd for the IPC handler closure
    let hwnd_for_ipc = hwnd;
    
    let result = WebViewBuilder::new()
        .with_bounds(Rect {
            position: wry::dpi::Position::Physical(wry::dpi::PhysicalPosition::new(0, 0)),
            size: wry::dpi::Size::Physical(wry::dpi::PhysicalSize::new(
                (rect.right - rect.left) as u32,
                (rect.bottom - rect.top) as u32
            )),
        })
        .with_html(&html)
        .with_transparent(false)
        .with_ipc_handler(move |msg: wry::http::Request<String>| {
            let body = msg.body();
            if body == "startDrag" {
                // Initiate window drag
                unsafe {
                    let _ = ReleaseCapture();
                    SendMessageW(
                        hwnd_for_ipc,
                        WM_NCLBUTTONDOWN,
                        WPARAM(HTCAPTION as usize),
                        LPARAM(0)
                    );
                }
            } else if body == "close" {
                unsafe {
                    let _ = PostMessageW(
                        hwnd_for_ipc,
                        WM_CLOSE,
                        WPARAM(0),
                        LPARAM(0)
                    );
                }
            }
        })
        .build_as_child(&wrapper);
    
    if let Ok(webview) = result {
        REALTIME_WEBVIEWS.with(|wvs| {
            wvs.borrow_mut().insert(hwnd_key, webview);
        });
    }
}

fn destroy_realtime_webview(hwnd: HWND) {
    let hwnd_key = hwnd.0 as isize;
    REALTIME_WEBVIEWS.with(|wvs| {
        wvs.borrow_mut().remove(&hwnd_key);
    });
}

fn update_webview_text(hwnd: HWND, text: &str) {
    let hwnd_key = hwnd.0 as isize;
    
    // Escape the text for JavaScript
    let escaped = text
        .replace('\\', "\\\\")
        .replace('\'', "\\'")
        .replace('\n', "\\n")
        .replace('\r', "");
    
    let script = format!("window.updateText('{}');", escaped);
    
    REALTIME_WEBVIEWS.with(|wvs| {
        if let Some(webview) = wvs.borrow().get(&hwnd_key) {
            let _ = webview.evaluate_script(&script);
        }
    });
}

unsafe extern "system" fn realtime_wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_REALTIME_UPDATE => {
            // Get display text from state
            let text = {
                if let Ok(state) = REALTIME_STATE.lock() {
                    state.display_transcript.clone()
                } else {
                    String::new()
                }
            };
            update_webview_text(hwnd, &text);
            LRESULT(0)
        }
        WM_NCHITTEST => {
            // Allow dragging the window
            LRESULT(HTCAPTION as isize)
        }
        WM_CLOSE => {
            REALTIME_STOP_SIGNAL.store(true, Ordering::SeqCst);
            DestroyWindow(hwnd);
            
            if TRANSLATION_HWND.0 != 0 {
                DestroyWindow(TRANSLATION_HWND);
            }
            
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe extern "system" fn translation_wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_TRANSLATION_UPDATE => {
            // Get display text from state
            let text = {
                if let Ok(state) = REALTIME_STATE.lock() {
                    state.display_translation.clone()
                } else {
                    String::new()
                }
            };
            update_webview_text(hwnd, &text);
            LRESULT(0)
        }
        WM_NCHITTEST => {
            LRESULT(HTCAPTION as isize)
        }
        WM_CLOSE => {
            REALTIME_STOP_SIGNAL.store(true, Ordering::SeqCst);
            DestroyWindow(hwnd);
            
            if REALTIME_HWND.0 != 0 {
                DestroyWindow(REALTIME_HWND);
            }
            
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
