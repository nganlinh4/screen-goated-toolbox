//! WRY mini-app for 3D generation.
//!
//! The SGT window stays lightweight: it collects job options and shows status.
//! The heavy generation workflow is owned by an external helper runtime.

mod assets;
mod depth_model;
mod file_dialogs;
mod ipc;
mod runtime;
mod runtime_bundle;
mod window;

pub(crate) use depth_model::{
    DOWNLOAD_TITLE as DEPTH_DOWNLOAD_TITLE, depth_model_dir, download_depth_model,
    is_depth_model_downloaded, remove_depth_model,
};
pub(crate) use runtime_bundle::{
    DOWNLOAD_TITLE as RUNTIME_DOWNLOAD_TITLE, download_runtime, is_runtime_installed,
    remove_runtime, runtime_bundle_dir,
};

use std::sync::Once;

use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use wry::WebContext;

use crate::win_types::SendHwnd;

pub(super) const WM_APP_SHOW: u32 = WM_USER + 461;
pub(super) const WM_APP_SYNC: u32 = WM_USER + 462;

pub(super) static REGISTER_CLASS: Once = Once::new();
pub(super) static mut WINDOW_HWND: SendHwnd = SendHwnd(HWND(std::ptr::null_mut()));
pub(super) static mut IS_READY: bool = false;
pub(super) static mut IS_INITIALIZING: bool = false;

thread_local! {
    pub(super) static WEBVIEW: std::cell::RefCell<Option<wry::WebView>> =
        const { std::cell::RefCell::new(None) };
    pub(super) static WEB_CONTEXT: std::cell::RefCell<Option<WebContext>> =
        const { std::cell::RefCell::new(None) };
}

pub fn show_three_d_generator() {
    let capability = crate::runtime_support::require_webview2("3D Generator");
    if !capability.is_supported() {
        crate::runtime_support::notify_capability_issue(&capability);
        return;
    }
    let _ = runtime::prepare_runtime();
    window::show();
}

pub(super) fn current_ui_language() -> String {
    crate::APP
        .lock()
        .map(|app| app.config.ui_language.clone())
        .unwrap_or_else(|_| "en".to_string())
}

pub fn update_settings() {
    unsafe {
        if !IS_READY {
            return;
        }
        let hwnd = std::ptr::addr_of!(WINDOW_HWND).read();
        if !hwnd.is_invalid() {
            let _ = PostMessageW(Some(hwnd.0), WM_APP_SYNC, WPARAM(0), LPARAM(0));
        }
    }
}
