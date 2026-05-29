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

pub use file_dialogs::pick_audio_file_dialog as pick_step_audio_reference_audio;

use std::sync::Once;

use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use wry::WebContext;

use crate::win_types::SendHwnd;

pub(super) const WM_APP_SHOW: u32 = WM_USER + 401;
/// Posted when the host app's theme/language changes so the webview re-renders.
pub(super) const WM_APP_SYNC: u32 = WM_USER + 402;
/// Posted periodically while audio is playing so the player position advances.
pub(super) const WM_APP_TICK: u32 = WM_USER + 403;

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

pub fn show_tts_playground() {
    let capability = crate::runtime_support::require_webview2("TTS Playground");
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
