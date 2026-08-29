//! WRY mini-app for 3D generation.
//!
//! The SGT window stays lightweight: it collects job options and shows status.
//! The mini-app keeps creation, queueing, and result delivery in one product surface.

mod asset_protocol;
mod asset_texture_validation;
mod assets;
mod export;
pub(crate) mod file_dialogs;
mod ipc;
mod runtime;
mod window;

use std::sync::Once;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::win_types::SendHwnd;

pub(super) const WM_APP_SHOW: u32 = WM_USER + 461;
pub(super) const WM_APP_SYNC: u32 = WM_USER + 462;
pub(super) const WM_APP_PREVIEW_REPLY: u32 = WM_USER + 463;
pub(super) const WM_APP_REMOVE: u32 = WM_USER + 464;

pub(super) static REGISTER_CLASS: Once = Once::new();
pub(super) static mut WINDOW_HWND: SendHwnd = SendHwnd(HWND(std::ptr::null_mut()));
pub(super) static mut IS_READY: bool = false;
pub(super) static mut IS_INITIALIZING: bool = false;
pub(super) static REMOVAL_REQUESTED: AtomicBool = AtomicBool::new(false);

thread_local! {
    pub(super) static WEBVIEW: std::cell::RefCell<Option<wry::WebView>> =
        const { std::cell::RefCell::new(None) };
    pub(super) static WEB_CONTEXT: std::cell::RefCell<Option<crate::overlay::webview_runtime::ManagedContext>> =
        const { std::cell::RefCell::new(None) };
    pub(super) static ASSET_PACK: std::cell::RefCell<Option<crate::component_registry::creation::CreationPack>> =
        const { std::cell::RefCell::new(None) };
}

pub fn show_three_d_generator() {
    let capability = crate::runtime_support::require_webview2("3D Generator");
    if !capability.is_supported() {
        crate::runtime_support::notify_capability_issue(&capability);
        return;
    }
    crate::component_registry::creation::launch_when_ready(|| {
        let _ = runtime::prepare_runtime();
        window::show();
    });
}

pub(crate) fn product_dir() -> std::path::PathBuf {
    crate::component_registry::creation::component_dir()
}

pub(crate) fn is_product_installed() -> bool {
    crate::component_registry::creation::is_installed_for_display()
}

pub(crate) fn is_product_partially_installed() -> bool {
    crate::component_registry::creation::is_partially_installed()
}

pub(crate) fn is_product_available() -> bool {
    crate::component_registry::creation::is_available()
}

pub(crate) fn download_product(
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    use_badge: bool,
) -> anyhow::Result<()> {
    crate::component_registry::creation::download(stop, use_badge)
}

pub(crate) fn remove_product() -> anyhow::Result<()> {
    REMOVAL_REQUESTED.store(true, Ordering::Release);
    let result = stop_for_component_removal().and_then(|()| {
        crate::overlay::creation_runtime::remove_runtime()?;
        crate::component_registry::creation::remove_legacy_components()
    });
    REMOVAL_REQUESTED.store(false, Ordering::Release);
    result
}

fn stop_for_component_removal() -> anyhow::Result<()> {
    let _ = shutdown();
    crate::overlay::creation_runtime::stop_for_component_removal();
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let (hwnd, initializing) = unsafe {
            (
                std::ptr::addr_of!(WINDOW_HWND).read(),
                std::ptr::addr_of!(IS_INITIALIZING).read(),
            )
        };
        if !hwnd.is_invalid() {
            unsafe {
                let _ = PostMessageW(Some(hwnd.0), WM_APP_REMOVE, WPARAM(0), LPARAM(0));
            }
        } else if !initializing {
            return Ok(());
        }
        if Instant::now() >= deadline {
            anyhow::bail!("3D Creation did not stop before component removal timed out");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
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
