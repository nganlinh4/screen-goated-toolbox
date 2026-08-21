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

use crate::component_registry::web_assets::WebAssetComponent;
pub(crate) use crate::overlay::creation_runtime::{
    download_runtime, download_title as runtime_download_title, is_runtime_installed,
    runtime_bundle_dir,
};
use std::sync::Once;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use wry::WebContext;

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
    pub(super) static WEB_CONTEXT: std::cell::RefCell<Option<WebContext>> =
        const { std::cell::RefCell::new(None) };
    pub(super) static ASSET_PACK: std::cell::RefCell<Option<crate::component_registry::web_assets::WebAssetPack>> =
        const { std::cell::RefCell::new(None) };
}

pub fn show_three_d_generator() {
    let capability = crate::runtime_support::require_webview2("3D Generator");
    if !capability.is_supported() {
        crate::runtime_support::notify_capability_issue(&capability);
        return;
    }
    let _ = runtime::prepare_runtime();
    crate::component_registry::web_assets::launch_when_ready(
        WebAssetComponent::Creation3d,
        window::show,
    );
}

pub(crate) fn web_asset_download_title() -> String {
    crate::component_registry::web_assets::download_title(WebAssetComponent::Creation3d)
}

pub(crate) fn are_web_assets_installed() -> bool {
    crate::component_registry::web_assets::is_installed_for_display(WebAssetComponent::Creation3d)
}

pub(crate) fn web_assets_dir() -> std::path::PathBuf {
    crate::component_registry::web_assets::component_dir(WebAssetComponent::Creation3d)
}

pub(crate) fn download_web_assets(
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    use_badge: bool,
) -> anyhow::Result<()> {
    crate::component_registry::web_assets::download_from_manager(
        WebAssetComponent::Creation3d,
        stop,
        use_badge,
    )
}

pub(crate) fn remove_web_assets() -> anyhow::Result<()> {
    REMOVAL_REQUESTED.store(true, Ordering::Release);
    let result = stop_for_component_removal().and_then(|()| {
        crate::component_registry::web_assets::remove(WebAssetComponent::Creation3d)
    });
    REMOVAL_REQUESTED.store(false, Ordering::Release);
    result
}

pub(crate) fn remove_runtime() -> anyhow::Result<()> {
    REMOVAL_REQUESTED.store(true, Ordering::Release);
    let result = stop_for_component_removal()
        .and_then(|()| crate::overlay::creation_runtime::remove_runtime());
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
