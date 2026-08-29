use std::cell::Cell;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use anyhow::Result;
#[cfg(not(feature = "recorder-worker"))]
use anyhow::bail;
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

use crate::api::realtime_audio::WM_DOWNLOAD_PROGRESS;
#[cfg(not(feature = "recorder-worker"))]
use crate::component_registry::RemovalOutcome;
use crate::component_registry::models::{ModelKind, ModelUse};

pub(super) fn ensure_model(
    kind: ModelKind,
    stop: Arc<AtomicBool>,
    use_badge: bool,
    title: &str,
    message: &str,
) -> Result<()> {
    if crate::component_registry::models::is_installed(kind) {
        return Ok(());
    }
    let _activity = crate::install_activity::register(stop.clone())?;
    set_state(true, title, message, 0.0);
    let badge = use_badge
        .then(|| crate::overlay::auto_copy_badge::DownloadProgressBadge::with_text(title, message));
    let last_percent = Cell::new(u32::MAX);
    let result = crate::component_registry::models::ensure(kind, &stop, |done, total| {
        let percent = done
            .saturating_mul(100)
            .checked_div(total)
            .unwrap_or(0)
            .min(100) as u32;
        if last_percent.replace(percent) == percent {
            return;
        }
        set_state(true, title, message, percent as f32);
        if let Some(badge) = &badge {
            badge.report(percent as u64, 100);
        }
    });
    set_state(
        false,
        title,
        message,
        if result.is_ok() { 100.0 } else { 0.0 },
    );
    if let Some(badge) = &badge {
        badge.finish();
    }
    result.map(drop)
}

pub(super) fn acquire_model(kind: ModelKind) -> Result<ModelUse> {
    crate::component_registry::models::acquire_installed(kind)
}

#[cfg(not(feature = "recorder-worker"))]
pub(super) fn installed_size(kind: ModelKind) -> u64 {
    crate::component_registry::models::installed_size(kind)
}

#[cfg(not(feature = "recorder-worker"))]
pub(super) fn remove_model(kind: ModelKind) -> Result<()> {
    let _owners = crate::overlay::component_removal::stop_audio_owners()?;
    match crate::component_registry::models::remove(kind)? {
        RemovalOutcome::Missing | RemovalOutcome::Removed | RemovalOutcome::Pending => Ok(()),
        RemovalOutcome::RequiredBy(dependents) => {
            bail!("model is required by {}", dependents.join(", "))
        }
        RemovalOutcome::PreservedModified(paths) => bail!(
            "unrecorded or unsafe model content was preserved: {}",
            paths
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

fn set_state(downloading: bool, title: &str, message: &str, progress: f32) {
    use crate::overlay::realtime_webview::state::REALTIME_STATE;
    if let Ok(mut state) = REALTIME_STATE.lock() {
        state.is_downloading = downloading;
        state.download_title = title.to_string();
        state.download_message = message.to_string();
        state.download_progress = progress;
    }
    unsafe {
        use crate::overlay::realtime_webview::state::REALTIME_HWND;
        if !std::ptr::addr_of!(REALTIME_HWND).read().is_invalid() {
            let _ = PostMessageW(
                Some(REALTIME_HWND),
                WM_DOWNLOAD_PROGRESS,
                WPARAM(0),
                LPARAM(0),
            );
        }
    }
}
