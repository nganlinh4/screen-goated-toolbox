//! WRY mini-app for creating or editing an image from a reference.

mod assets;
mod ipc;
mod runtime;
mod window;

use std::sync::Once;

use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, w};
use wry::WebContext;

use crate::win_types::SendHwnd;

pub(super) const WM_APP_SHOW: u32 = WM_USER + 481;
pub(super) const WM_APP_SYNC: u32 = WM_USER + 482;
pub(super) const WINDOW_SIZE_FILE: &str = "image-creator-window.json";

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

pub fn show_image_creator() {
    let capability = crate::runtime_support::require_webview2("Create/edit image");
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

pub(super) fn window_class_name() -> PCWSTR {
    w!("ImageCreatorWindowClass")
}

pub(super) fn window_title() -> String {
    crate::gui::locale::LocaleText::get(&current_ui_language())
        .shell
        .image_creator_title
        .to_string()
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
