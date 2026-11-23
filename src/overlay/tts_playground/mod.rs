//! WRY mini-app for the TTS Playground. The audio backends stay behind
//! `TTS_MANAGER`; this module owns only the window, its state and its IPC.

mod assets;
mod catalogs;
mod draft_persistence;
mod exit_drain;
mod file_dialogs;
mod ipc;
mod library;
mod lifecycle;
mod runtime;
mod runtime_clips;
mod runtime_generation;
mod runtime_playback;
mod runtime_sources;
mod state;
mod window;

pub(crate) use exit_drain::ExitDrainGuard;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard, Once};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::component_registry::web_assets::WebAssetComponent;

pub(super) const WM_APP_SHOW: u32 = WM_USER + 401;
/// Posted when the host app's theme/language changes so the webview re-renders.
pub(super) const WM_APP_SYNC: u32 = WM_USER + 402;
/// Posted periodically while audio is playing so the player position advances.
pub(super) const WM_APP_TICK: u32 = WM_USER + 403;
pub(super) const WM_APP_REMOVE: u32 = WM_USER + 404;
pub(super) const WM_APP_EXIT_DRAIN: u32 = WM_USER + 405;
pub(super) const WM_APP_EXIT_THAW: u32 = WM_USER + 406;

pub(super) static REGISTER_CLASS: Once = Once::new();
static WINDOW_LIFECYCLE: Mutex<lifecycle::WindowLifecycle> =
    Mutex::new(lifecycle::WindowLifecycle::new());
static REMOVAL_LOCK: Mutex<()> = Mutex::new(());
pub(super) static REMOVAL_ADMISSION: Mutex<()> = Mutex::new(());
static NEXT_REMOVAL: AtomicUsize = AtomicUsize::new(1);
pub(super) static ACTIVE_REMOVAL: AtomicUsize = AtomicUsize::new(0);
pub(super) static FAILED_REMOVAL: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    pub(super) static WEBVIEW: std::cell::RefCell<Option<wry::WebView>> =
        const { std::cell::RefCell::new(None) };
    pub(super) static WEB_CONTEXT: std::cell::RefCell<Option<crate::overlay::webview_runtime::ManagedContext>> =
        const { std::cell::RefCell::new(None) };
    pub(super) static ASSET_PACK: std::cell::RefCell<Option<crate::component_registry::web_assets::WebAssetPack>> =
        const { std::cell::RefCell::new(None) };
}

pub fn show_tts_playground() {
    let capability = crate::runtime_support::require_webview2(
        crate::runtime_support::WebViewFeature::TtsPlayground,
    );
    if !capability.is_supported() {
        crate::runtime_support::notify_capability_issue(&capability);
        return;
    }
    crate::component_registry::web_assets::launch_when_ready(
        WebAssetComponent::TtsPlayground,
        window::show,
    );
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
    let _guard = stop_for_component_removal()?;
    crate::component_registry::web_assets::remove(WebAssetComponent::TtsPlayground)
}

pub(crate) struct ComponentRemovalGuard {
    _lock: MutexGuard<'static, ()>,
}

impl Drop for ComponentRemovalGuard {
    fn drop(&mut self) {
        WINDOW_LIFECYCLE
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .end_removal();
    }
}

pub(crate) fn stop_for_component_removal() -> anyhow::Result<ComponentRemovalGuard> {
    let lock = REMOVAL_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    with_page_drain(exit_drain::drain, || stop_after_page_drain(lock))
}

fn with_page_drain<G, R>(
    drain: impl FnOnce() -> std::io::Result<G>,
    operation: impl FnOnce() -> anyhow::Result<R>,
) -> anyhow::Result<R> {
    let _page_guard = drain()?;
    operation()
}

fn stop_after_page_drain(lock: MutexGuard<'static, ()>) -> anyhow::Result<ComponentRemovalGuard> {
    {
        let _admission = REMOVAL_ADMISSION
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        WINDOW_LIFECYCLE
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .begin_removal();
    }
    let guard = ComponentRemovalGuard { _lock: lock };
    let request = NEXT_REMOVAL
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
        .map_err(|_| anyhow::anyhow!("TTS Playground removal request identity exhausted"))?;
    ACTIVE_REMOVAL.store(request, Ordering::Release);
    struct ActiveRemoval;
    impl Drop for ActiveRemoval {
        fn drop(&mut self) {
            let _admission = REMOVAL_ADMISSION
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            ACTIVE_REMOVAL.store(0, Ordering::Release);
        }
    }
    let _active = ActiveRemoval;
    wait_for_removal(
        Instant::now() + Duration::from_secs(5),
        || {
            WINDOW_LIFECYCLE
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .removal_observation()
        },
        |hwnd| unsafe {
            PostMessageW(
                Some(HWND(hwnd as *mut std::ffi::c_void)),
                WM_APP_REMOVE,
                WPARAM(request),
                LPARAM(0),
            )
            .map_err(|error| anyhow::anyhow!("could not request TTS Playground removal: {error}"))
        },
        || FAILED_REMOVAL.load(Ordering::Acquire) == request,
        || std::thread::sleep(Duration::from_millis(20)),
    )?;
    Ok(guard)
}

fn wait_for_removal(
    deadline: Instant,
    mut observe: impl FnMut() -> (Option<isize>, bool),
    mut post: impl FnMut(isize) -> anyhow::Result<()>,
    mut failed: impl FnMut() -> bool,
    mut pause: impl FnMut(),
) -> anyhow::Result<()> {
    let mut posted_window = None;
    loop {
        if Instant::now() >= deadline {
            anyhow::bail!("TTS Playground did not stop before component removal timed out");
        }
        let (hwnd, initializing) = observe();
        if failed() {
            anyhow::bail!("TTS Playground draft could not be saved before component removal");
        }
        // WebView setup may pump messages reentrantly. Do not destroy the
        // native window until setup has finished using it.
        if !initializing {
            if let Some(hwnd) = hwnd {
                if posted_window != Some(hwnd) {
                    post(hwnd)?;
                    posted_window = Some(hwnd);
                }
            } else {
                return Ok(());
            }
        }
        pause();
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
    let hwnd = WINDOW_LIFECYCLE
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .ready_window();
    if let Some((hwnd, generation)) = hwnd {
        unsafe {
            let _ = PostMessageW(
                Some(HWND(hwnd as *mut std::ffi::c_void)),
                WM_APP_SYNC,
                WPARAM(generation),
                LPARAM(0),
            );
        }
    }
}

fn flush_draft_or_notify() -> bool {
    if let Err(error) = draft_persistence::flush() {
        crate::log_info!("[TTS Playground] Could not save draft: {error}");
        let language = current_ui_language();
        let text = crate::gui::locale::LocaleText::get(&language);
        crate::overlay::auto_copy_badge::show_notification(text.shell.settings_save_failed_title);
        return false;
    }
    true
}

pub(crate) fn freeze_draft_for_exit() -> std::io::Result<()> {
    draft_persistence::freeze_and_flush()
}

pub(crate) fn thaw_draft_after_aborted_exit() {
    draft_persistence::thaw_after_aborted_exit();
}

pub(crate) fn drain_draft_for_exit() -> std::io::Result<ExitDrainGuard> {
    exit_drain::drain()
}

#[cfg(test)]
mod removal_tests;
