use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use std::sync::Mutex;
use std::collections::HashMap;
use std::num::NonZeroIsize;
use pulldown_cmark::{Parser, Options, html};
use wry::{WebViewBuilder, Rect};
use raw_window_handle::{HasWindowHandle, RawWindowHandle, WindowHandle, Win32WindowHandle, HandleError};

lazy_static::lazy_static! {
    // Store WebViews per parent window - wrapped in thread-local storage to avoid Send issues
    static ref WEBVIEW_STATES: Mutex<HashMap<isize, bool>> = Mutex::new(HashMap::new());
}

// Thread-local storage for WebViews since they're not Send
thread_local! {
    static WEBVIEWS: std::cell::RefCell<HashMap<isize, wry::WebView>> = std::cell::RefCell::new(HashMap::new());
}

/// Wrapper for HWND to implement HasWindowHandle
struct HwndWrapper(HWND);

impl HasWindowHandle for HwndWrapper {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let hwnd = self.0.0 as isize;
        if let Some(non_zero) = NonZeroIsize::new(hwnd) {
            let mut handle = Win32WindowHandle::new(non_zero);
            // hinstance is optional, can be null
            handle.hinstance = None;
            let raw = RawWindowHandle::Win32(handle);
            // Safety: the handle is valid for the lifetime of HwndWrapper
            Ok(unsafe { WindowHandle::borrow_raw(raw) })
        } else {
            Err(HandleError::Unavailable)
        }
    }
}

/// CSS styling for the markdown content
const MARKDOWN_CSS: &str = r#"
    * { box-sizing: border-box; }
    body { 
        font-family: 'Segoe UI', -apple-system, sans-serif;
        font-size: 14px;
        line-height: 1.6;
        background: linear-gradient(180deg, #1e1e1e 0%, #151515 100%);
        color: #e0e0e0;
        padding: 8px;
        margin: 0;
        overflow-x: hidden;
        word-wrap: break-word;
    }
    body > *:first-child { margin-top: 0; }
    h1 { font-size: 1.8em; color: #4fc3f7; border-bottom: 1px solid #333; padding-bottom: 8px; margin-top: 0; }
    h2 { font-size: 1.5em; color: #81d4fa; border-bottom: 1px solid #2a2a2a; padding-bottom: 6px; margin-top: 0.5em; }
    h3 { font-size: 1.2em; color: #b3e5fc; margin-top: 0.5em; }
    h4, h5, h6 { color: #e1f5fe; margin-top: 0.5em; }
    p { margin: 0.5em 0; }
    code { 
        font-family: 'Cascadia Code', 'Fira Code', Consolas, monospace;
        background: #2d2d2d; 
        padding: 2px 6px; 
        border-radius: 4px;
        font-size: 0.9em;
        color: #ce9178;
    }
    pre { 
        background: #1a1a1a; 
        padding: 12px 16px; 
        border-radius: 8px; 
        overflow-x: auto;
        border: 1px solid #333;
    }
    pre code { 
        background: transparent; 
        padding: 0; 
        color: #d4d4d4;
    }
    a { color: #81d4fa; text-decoration: none; }
    a:hover { text-decoration: underline; }
    blockquote { 
        border-left: 4px solid #4fc3f7; 
        padding-left: 16px; 
        margin-left: 0;
        color: #aaa; 
        background: #1a1a1a;
        padding: 8px 16px;
        border-radius: 0 8px 8px 0;
    }
    ul, ol { padding-left: 24px; margin: 0.8em 0; }
    li { margin: 4px 0; }
    table { 
        border-collapse: collapse; 
        width: 100%; 
        margin: 1em 0;
    }
    th, td { 
        border: 1px solid #444; 
        padding: 8px 12px; 
        text-align: left;
    }
    th { background: #252525; color: #81d4fa; }
    tr:nth-child(even) { background: #1a1a1a; }
    hr { border: none; border-top: 1px solid #444; margin: 1.5em 0; }
    img { max-width: 100%; border-radius: 8px; }
    
    /* Scrollbar styling */
    ::-webkit-scrollbar { width: 8px; height: 8px; }
    ::-webkit-scrollbar-track { background: #1a1a1a; }
    ::-webkit-scrollbar-thumb { background: #444; border-radius: 4px; }
    ::-webkit-scrollbar-thumb:hover { background: #555; }
"#;

/// Convert markdown text to styled HTML
pub fn markdown_to_html(markdown: &str) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    
    let parser = Parser::new_ext(markdown, options);
    let mut html_output = String::new();
    html::push_html(&mut html_output, parser);
    
    format!(
        r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <style>{}</style>
</head>
<body>{}</body>
</html>"#,
        MARKDOWN_CSS,
        html_output
    )
}

/// Create a WebView child window for markdown rendering
/// Must be called from the main thread!
pub fn create_markdown_webview(parent_hwnd: HWND, markdown_text: &str) -> bool {
    let hwnd_key = parent_hwnd.0 as isize;
    
    // Check if we already have a webview
    let exists = WEBVIEWS.with(|webviews| {
        webviews.borrow().contains_key(&hwnd_key)
    });
    
    if exists {
        return update_markdown_content(parent_hwnd, markdown_text);
    }
    
    // Get parent window rect
    let mut rect = RECT::default();
    unsafe { GetClientRect(parent_hwnd, &mut rect); }
    
    let html_content = markdown_to_html(markdown_text);
    let wrapper = HwndWrapper(parent_hwnd);
    
    // Small margin on edges for resize handle accessibility (2px)
    // 52px at bottom for buttons (btn_size 28 + margin 12 * 2)
    let edge_margin = 2.0;
    let button_area_height = 52.0;
    let content_width = ((rect.right - rect.left) as f64 - edge_margin * 2.0).max(50.0);
    let content_height = ((rect.bottom - rect.top) as f64 - edge_margin - button_area_height).max(50.0);
    
    // Create WebView with small margins so resize handles remain accessible
    let result = WebViewBuilder::new()
        .with_bounds(Rect {
            position: wry::dpi::Position::Logical(wry::dpi::LogicalPosition::new(edge_margin, edge_margin)),
            size: wry::dpi::Size::Logical(wry::dpi::LogicalSize::new(
                content_width,
                content_height
            )),
        })
        .with_html(&html_content)
        .with_transparent(false)
        .build_as_child(&wrapper);
    
    match result {
        Ok(webview) => {
            WEBVIEWS.with(|webviews| {
                webviews.borrow_mut().insert(hwnd_key, webview);
            });
            
            let mut states = WEBVIEW_STATES.lock().unwrap();
            states.insert(hwnd_key, true);
            true
        }
        Err(e) => {
            eprintln!("Failed to create WebView: {:?}", e);
            false
        }
    }
}

/// Update the markdown content in an existing WebView
pub fn update_markdown_content(parent_hwnd: HWND, markdown_text: &str) -> bool {
    let hwnd_key = parent_hwnd.0 as isize;
    let html = markdown_to_html(markdown_text);
    
    WEBVIEWS.with(|webviews| {
        if let Some(webview) = webviews.borrow().get(&hwnd_key) {
            // Navigate to the new HTML content using JavaScript
            let escaped_html = html.replace('\\', "\\\\")
                .replace('`', "\\`")
                .replace("${", "\\${");
            let script = format!(
                "document.open(); document.write(`{}`); document.close();",
                escaped_html
            );
            let _ = webview.evaluate_script(&script);
            return true;
        }
        false
    })
}

/// Resize the WebView to match parent window
/// When hovered: leaves 52px at bottom for buttons
/// When not hovered: expands to full height for clean view
pub fn resize_markdown_webview(parent_hwnd: HWND, is_hovered: bool) {
    let hwnd_key = parent_hwnd.0 as isize;
    
    unsafe {
        let mut rect = RECT::default();
        GetClientRect(parent_hwnd, &mut rect);
        
        // 2px edge margin for resize handles
        let edge_margin = 2.0;
        // Only reserve button area when hovered
        let button_area_height = if is_hovered { 52.0 } else { edge_margin };
        
        let content_width = ((rect.right - rect.left) as f64 - edge_margin * 2.0).max(50.0);
        let content_height = ((rect.bottom - rect.top) as f64 - edge_margin - button_area_height).max(50.0);
        
        WEBVIEWS.with(|webviews| {
            if let Some(webview) = webviews.borrow().get(&hwnd_key) {
                let _ = webview.set_bounds(Rect {
                    position: wry::dpi::Position::Logical(wry::dpi::LogicalPosition::new(edge_margin, edge_margin)),
                    size: wry::dpi::Size::Logical(wry::dpi::LogicalSize::new(
                        content_width,
                        content_height
                    )),
                });
            }
        });
    }
}

/// Hide the WebView (toggle back to plain text)
pub fn hide_markdown_webview(parent_hwnd: HWND) {
    let hwnd_key = parent_hwnd.0 as isize;
    
    WEBVIEWS.with(|webviews| {
        if let Some(webview) = webviews.borrow().get(&hwnd_key) {
            let _ = webview.set_visible(false);
        }
    });
}

/// Show the WebView (toggle to markdown mode)
pub fn show_markdown_webview(parent_hwnd: HWND) {
    let hwnd_key = parent_hwnd.0 as isize;
    
    WEBVIEWS.with(|webviews| {
        if let Some(webview) = webviews.borrow().get(&hwnd_key) {
            let _ = webview.set_visible(true);
        }
    });
}

/// Destroy the WebView when window closes
pub fn destroy_markdown_webview(parent_hwnd: HWND) {
    let hwnd_key = parent_hwnd.0 as isize;
    
    WEBVIEWS.with(|webviews| {
        webviews.borrow_mut().remove(&hwnd_key);
    });
    
    let mut states = WEBVIEW_STATES.lock().unwrap();
    states.remove(&hwnd_key);
}

/// Check if markdown webview exists for this window
pub fn has_markdown_webview(parent_hwnd: HWND) -> bool {
    let hwnd_key = parent_hwnd.0 as isize;
    let states = WEBVIEW_STATES.lock().unwrap();
    states.get(&hwnd_key).copied().unwrap_or(false)
}
