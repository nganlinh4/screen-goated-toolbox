use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Result, anyhow};

use super::{clear_runtime_notice, runtime_locale, set_runtime_notice};

static RUNTIME_DOWNLOAD_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

#[cfg(not(feature = "recorder-worker"))]
pub fn is_qwen3_runtime_downloading() -> bool {
    RUNTIME_DOWNLOAD_IN_PROGRESS.load(Ordering::Relaxed)
}

pub fn is_qwen3_runtime_managed_installed() -> bool {
    crate::component_registry::qwen_runtime::is_installed()
}

#[cfg(not(feature = "recorder-worker"))]
pub fn is_qwen3_runtime_installed_for_display() -> bool {
    crate::component_registry::qwen_runtime::is_installed_for_display()
}

#[cfg(not(feature = "recorder-worker"))]
pub fn qwen3_runtime_installed_size() -> u64 {
    crate::component_registry::qwen_runtime::installed_size()
}

#[cfg(not(feature = "recorder-worker"))]
pub fn qwen3_runtime_installed_size_for_display() -> u64 {
    crate::component_registry::qwen_runtime::installed_size_for_display()
}

#[cfg(not(feature = "recorder-worker"))]
pub fn remove_qwen3_runtime() -> Result<()> {
    let result = crate::component_registry::qwen_runtime::remove();
    if let Err(error) = &result {
        set_runtime_notice(error.to_string());
    } else {
        clear_runtime_notice();
    }
    result
}

pub fn download_qwen3_runtime(stop_signal: Arc<AtomicBool>, use_badge: bool) -> Result<()> {
    if is_qwen3_runtime_managed_installed() {
        return Ok(());
    }
    if RUNTIME_DOWNLOAD_IN_PROGRESS
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        while RUNTIME_DOWNLOAD_IN_PROGRESS.load(Ordering::Acquire) {
            std::thread::sleep(std::time::Duration::from_millis(300));
            if stop_signal.load(Ordering::Relaxed) {
                return Err(anyhow!("Download cancelled while waiting"));
            }
        }
        return is_qwen3_runtime_managed_installed()
            .then_some(())
            .ok_or_else(|| anyhow!("Qwen3 runtime download did not complete successfully"));
    }

    let locale = runtime_locale();
    let title = locale
        .tool_runtime
        .qwen3_runtime_downloading_title
        .to_string();
    use crate::overlay::realtime_webview::state::REALTIME_STATE;
    if let Ok(mut state) = REALTIME_STATE.lock() {
        state.is_downloading = true;
        state.download_title = title.clone();
        state.download_message = locale
            .tool_runtime
            .qwen3_runtime_downloading_message
            .to_string();
        state.download_progress = 0.0;
    }
    clear_runtime_notice();
    post_download_state();
    let badge = use_badge.then(|| {
        crate::overlay::auto_copy_badge::DownloadProgressBadge::with_text(
            &title,
            locale.tool_runtime.qwen3_runtime_downloading_message,
        )
    });

    let result = crate::component_registry::qwen_runtime::ensure_component(
        &stop_signal,
        |downloaded, total| {
            let progress = downloaded.saturating_mul(10_000) / total.max(1);
            let progress = progress.min(10_000) as f32 / 100.0;
            if let Ok(mut state) = REALTIME_STATE.lock() {
                state.download_progress = progress;
            }
            if let Some(badge) = &badge {
                badge.report(downloaded, total);
            }
            post_download_state();
        },
    )
    .map(|_| ());

    RUNTIME_DOWNLOAD_IN_PROGRESS.store(false, Ordering::Release);
    if let Ok(mut state) = REALTIME_STATE.lock() {
        state.is_downloading = false;
    }
    if let Some(badge) = &badge {
        badge.finish();
    }
    post_download_state();
    match &result {
        Ok(()) => clear_runtime_notice(),
        Err(error) if !error.to_string().contains("cancelled") => {
            set_runtime_notice(error.to_string());
        }
        Err(_) => {}
    }
    result
}

fn post_download_state() {
    use crate::overlay::realtime_webview::REALTIME_HWND;
    unsafe {
        if !std::ptr::addr_of!(REALTIME_HWND).read().is_invalid() {
            let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                Some(REALTIME_HWND),
                crate::api::realtime_audio::WM_DOWNLOAD_PROGRESS,
                windows::Win32::Foundation::WPARAM(0),
                windows::Win32::Foundation::LPARAM(0),
            );
        }
    }
}
