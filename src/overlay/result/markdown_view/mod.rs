//! Markdown view module - WebView-based markdown rendering
//!
//! This module handles rendering markdown content in WebView windows,
//! including streaming updates, navigation, and file operations.

use std::collections::HashMap;
use std::sync::Mutex;

// Sub-modules
pub mod conversion;
pub mod css;
pub mod file_ops;
pub mod html_utils;
pub mod ipc;
pub mod navigation;
pub mod streaming;
pub mod webview;

// Static state
lazy_static::lazy_static! {
    /// Store WebViews per parent window - wrapped in thread-local storage to avoid Send issues
    pub(crate) static ref WEBVIEW_STATES: Mutex<HashMap<isize, bool>> = Mutex::new(HashMap::new());
    /// Global flag to indicate WebView2 is ready
    #[allow(dead_code)]
    static ref WEBVIEW_READY: Mutex<bool> = Mutex::new(false);
    /// Flag to skip next navigation handler call (set before history.back())
    pub(crate) static ref SKIP_NEXT_NAVIGATION: Mutex<HashMap<isize, bool>> = Mutex::new(HashMap::new());
}

// Thread-local storage for WebViews since they're not Send
thread_local! {
    pub(crate) static WEBVIEWS: std::cell::RefCell<std::collections::HashMap<isize, wry::WebView>> = std::cell::RefCell::new(std::collections::HashMap::new());
    /// Shared WebContext for all WebViews on this thread - reduces RAM by sharing browser processes
    pub(crate) static SHARED_WEB_CONTEXT: std::cell::RefCell<Option<wry::WebContext>> = std::cell::RefCell::new(None);
}

// Re-exports for public API
pub use file_ops::save_html_file;
pub use navigation::{go_back, go_forward, update_markdown_content, update_markdown_content_ex};
pub use streaming::{
    fit_font_to_window, init_gridjs, reset_stream_counter, set_body_opacity,
    stream_markdown_content,
};
pub use webview::{
    create_markdown_webview, destroy_markdown_webview, has_markdown_webview, hide_markdown_webview,
    resize_markdown_webview, show_markdown_webview,
};
