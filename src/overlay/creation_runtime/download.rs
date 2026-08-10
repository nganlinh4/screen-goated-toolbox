use std::io::{Read as _, Write as _};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use anyhow::{Result, anyhow, bail};

use super::{
    RUNTIME_DELIVERY, cleanup_runtime_files, download_title, ensure_runtime_bundle_dir,
    invalidate_verified_runtime, is_reparse_point, is_runtime_installed, localized_component_name,
    runtime_bundle_dir, runtime_exe_path, runtime_shutting_down, validate_runtime,
    verified_installed_runtime_path, write_runtime_receipt,
};

fn download_delivery_file(
    url: &str,
    target: &Path,
    expected_bytes: u64,
    stop: &AtomicBool,
    on_progress: impl Fn(u64, u64),
) -> Result<()> {
    let response = crate::api::client::UREQ_DOWNLOAD_AGENT
        .get(url)
        .header("User-Agent", "ScreenGoatedToolbox")
        .call()
        .map_err(|error| anyhow!("Creation engine download failed: {error}"))?;
    let declared_bytes = response
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());
    if declared_bytes.is_some_and(|declared| declared != expected_bytes) {
        bail!("Creation engine download size does not match this build.");
    }
    let mut reader = response.into_body().into_reader();
    let mut output = std::fs::File::create(target)?;
    let mut buffer = [0_u8; 128 * 1024];
    let mut downloaded = 0_u64;
    loop {
        if stop.load(Ordering::Relaxed) || runtime_shutting_down() {
            bail!("Creation engine download was cancelled.");
        }
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        downloaded = downloaded
            .checked_add(read as u64)
            .ok_or_else(|| anyhow!("Creation engine download is too large."))?;
        if downloaded > expected_bytes {
            bail!("Creation engine download is larger than this build allows.");
        }
        output.write_all(&buffer[..read])?;
        on_progress(downloaded, expected_bytes);
    }
    if downloaded != expected_bytes {
        bail!("Creation engine download is incomplete.");
    }
    output.flush()?;
    output.sync_all()?;
    on_progress(downloaded, expected_bytes);
    Ok(())
}

pub(crate) fn download_runtime(stop: Arc<AtomicBool>, use_badge: bool) -> Result<()> {
    use crate::overlay::auto_copy_badge::{
        DownloadProgressBadge, NotificationType, show_detailed_notification,
        show_error_notification,
    };
    use crate::overlay::realtime_webview::state::REALTIME_STATE;

    static DOWNLOAD_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let _guard = DOWNLOAD_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|value| value.into_inner());
    if is_runtime_installed() {
        return Ok(());
    }
    let delivery = RUNTIME_DELIVERY
        .as_ref()
        .ok_or_else(|| anyhow!("Creation engine is not included in this build."))?;

    let path = runtime_exe_path();
    let partial = runtime_bundle_dir().join(format!("{}.download", delivery.asset));
    ensure_runtime_bundle_dir()?;
    let runtime_dir_metadata = std::fs::symlink_metadata(runtime_bundle_dir())?;
    if !runtime_dir_metadata.is_dir() || is_reparse_point(&runtime_dir_metadata) {
        bail!("Creation engine folder is not a regular directory.");
    }
    cleanup_runtime_files(false)?;
    invalidate_verified_runtime();
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    if partial.exists() {
        std::fs::remove_file(&partial)?;
    }

    let badge = crate::overlay::auto_copy_badge::locale_text();
    let component_name = localized_component_name();
    let title = download_title();
    let preparing = crate::overlay::auto_copy_badge::format_locale(
        badge.preparing_component_fmt,
        &[("name", &component_name)],
    );
    if let Ok(mut state) = REALTIME_STATE.lock() {
        state.is_downloading = true;
        state.download_title = title.clone();
        state.download_message = preparing.clone();
        state.download_progress = 0.0;
    }
    let progress_badge = use_badge.then(|| DownloadProgressBadge::with_text(&title, &preparing));

    let result = download_delivery_file(
        delivery.download_url,
        &partial,
        delivery.size_bytes,
        &stop,
        |downloaded, total| {
            let progress = if total > 0 {
                downloaded as f32 / total as f32 * 100.0
            } else {
                0.0
            };
            if let Ok(mut state) = REALTIME_STATE.lock() {
                state.download_message = title.clone();
                state.download_progress = progress;
            }
            if let Some(progress_badge) = &progress_badge {
                progress_badge.report(downloaded, total);
            }
        },
    )
    .and_then(|()| validate_runtime(&partial))
    .and_then(|()| {
        std::fs::rename(&partial, &path)
            .map_err(|error| anyhow!("Could not install creation engine: {error}"))
    })
    .and_then(|()| write_runtime_receipt())
    .and_then(|()| verified_installed_runtime_path().map(|_| ()));

    if result.is_err() {
        let _ = std::fs::remove_file(&partial);
    }
    if let Ok(mut state) = REALTIME_STATE.lock() {
        state.is_downloading = false;
        state.download_progress = if result.is_ok() { 100.0 } else { 0.0 };
    }
    if let Some(progress_badge) = &progress_badge {
        progress_badge.finish();
    }
    if use_badge {
        if result.is_ok() {
            let installed = crate::overlay::auto_copy_badge::format_locale(
                badge.component_installed_fmt,
                &[("name", &component_name)],
            );
            show_detailed_notification(&installed, &component_name, NotificationType::Success);
        } else {
            let failed = crate::overlay::auto_copy_badge::format_locale(
                badge.component_install_failed_fmt,
                &[("name", &component_name)],
            );
            show_error_notification(&failed);
        }
    }
    result
}
