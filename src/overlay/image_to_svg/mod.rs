//! WRY mini-app for turning raster images into editable SVG artwork.

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

pub(super) const WM_APP_SHOW: u32 = WM_USER + 471;
pub(super) const WM_APP_SYNC: u32 = WM_USER + 472;
pub(super) const WM_APP_PREVIEW_REPLY: u32 = WM_USER + 473;
pub(super) const WINDOW_SIZE_FILE: &str = "image-to-svg-window.json";

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

fn admit_dropped_images(paths: Vec<std::path::PathBuf>) -> Result<Vec<std::path::PathBuf>, String> {
    crate::overlay::three_d_generator::file_dialogs::admit_image_paths(
        paths,
        crate::overlay::three_d_generator::file_dialogs::MAX_BATCH_IMAGES,
        crate::overlay::three_d_generator::file_dialogs::MAX_BATCH_BYTES,
    )
}

pub fn show_image_to_svg() {
    if !crate::creation_feature_availability::request_image_to_svg_entry() {
        return;
    }
    let capability = crate::runtime_support::require_webview2("Image to SVG");
    if !capability.is_supported() {
        crate::runtime_support::notify_capability_issue(&capability);
        return;
    }
    window::show();
}

pub(super) fn window_class_name() -> PCWSTR {
    w!("ImageToSvgWindowClass")
}

pub(super) fn window_title() -> String {
    crate::gui::locale::LocaleText::get(&current_ui_language())
        .shell
        .image_to_svg_title
        .to_string()
}

pub(super) fn current_ui_language() -> String {
    crate::APP
        .lock()
        .map(|app| app.config.ui_language.clone())
        .unwrap_or_else(|_| "en".to_string())
}

pub(super) fn product_key() -> &'static str {
    "svg"
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
    crate::overlay::creation_close::begin_product(product_key());
    crate::overlay::creation_runtime::cancel_readiness("svg");
    let _ = runtime::cancel_for_shutdown();
    crate::overlay::creation_delivery::cancel_product_intents(product_key())
}
