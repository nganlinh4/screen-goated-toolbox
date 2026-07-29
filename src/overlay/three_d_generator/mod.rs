//! WRY mini-app for 3D generation.
//!
//! The SGT window stays lightweight: it collects job options and shows status.
//! The mini-app keeps creation, queueing, and result delivery in one product surface.

mod asset_protocol;
mod asset_texture_validation;
mod assets;
pub(crate) mod file_dialogs;
mod ipc;
mod runtime;
mod window;

pub(crate) use crate::overlay::creation_runtime::{
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
pub(super) const WM_APP_PREVIEW_REPLY: u32 = WM_USER + 463;

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

pub fn shutdown() -> bool {
    crate::overlay::creation_close::begin_product("3d");
    crate::overlay::creation_runtime::cancel_readiness("3d");
    let _ = runtime::cancel_for_shutdown();
    crate::overlay::creation_delivery::cancel_product_intents("3d")
}
