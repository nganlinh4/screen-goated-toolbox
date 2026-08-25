//! WRY mini-app for the TTS Playground. Replaces the old egui modal under
//! `gui/settings_ui/tts_playground/` for the surface; the audio backends
//! (TTS_MANAGER, magpie/kokoro/etc. runtimes) stay untouched.

mod assets;
mod catalogs;
pub mod file_dialogs;
mod ipc;
mod library;
mod runtime;
mod runtime_clips;
mod runtime_generation;
mod runtime_playback;
mod runtime_sources;
mod state;
mod window;

#[cfg(feature = "recorder-worker")]
pub use file_dialogs::pick_audio_file_dialog as pick_step_audio_reference_audio;

use std::sync::Once;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::component_registry::web_assets::WebAssetComponent;
use crate::win_types::SendHwnd;

pub(super) const WM_APP_SHOW: u32 = WM_USER + 401;
/// Posted when the host app's theme/language changes so the webview re-renders.
pub(super) const WM_APP_SYNC: u32 = WM_USER + 402;
/// Posted periodically while audio is playing so the player position advances.
pub(super) const WM_APP_TICK: u32 = WM_USER + 403;
pub(super) const WM_APP_REMOVE: u32 = WM_USER + 404;

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
    pub(super) static ASSET_PACK: std::cell::RefCell<Option<crate::component_registry::web_assets::WebAssetPack>> =
        const { std::cell::RefCell::new(None) };
}

pub fn show_tts_playground() {
    let capability = crate::runtime_support::require_webview2("TTS Playground");
    if !capability.is_supported() {
        crate::runtime_support::notify_capability_issue(&capability);
        return;
    }
    crate::component_registry::web_assets::launch_when_ready(
        WebAssetComponent::TtsPlayground,
        window::show,
    );
}

pub(crate) fn web_asset_download_title() -> String {
    crate::component_registry::web_assets::download_title(WebAssetComponent::TtsPlayground)
}

pub(crate) fn are_web_assets_installed() -> bool {
    crate::component_registry::web_assets::is_installed_for_display(
        WebAssetComponent::TtsPlayground,
    )
}

pub(crate) fn web_assets_dir() -> std::path::PathBuf {
    crate::component_registry::web_assets::component_dir(WebAssetComponent::TtsPlayground)
}

pub(crate) fn download_web_assets(
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    use_badge: bool,
) -> anyhow::Result<()> {
    crate::component_registry::web_assets::download_from_manager(
        WebAssetComponent::TtsPlayground,
        stop,
        use_badge,
    )
}

pub(crate) fn remove_web_assets() -> anyhow::Result<()> {
    REMOVAL_REQUESTED.store(true, Ordering::Release);
    let result = stop_for_component_removal().and_then(|()| {
        crate::component_registry::web_assets::remove(WebAssetComponent::TtsPlayground)
    });
    REMOVAL_REQUESTED.store(false, Ordering::Release);
    result
}

pub(crate) fn stop_for_component_removal() -> anyhow::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(5);
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
            anyhow::bail!("TTS Playground did not stop before component removal timed out");
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

/// Called by the host app when theme or UI language changes, so the open
/// playground window updates live. No-op if the window isn't open yet.
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
