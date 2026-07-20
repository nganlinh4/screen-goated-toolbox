//! Managed native sidecar used by the 3D Generator mini app.

use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, OnceLock};

use anyhow::{Result, anyhow, bail};
use sha2::{Digest, Sha256};

const RUNTIME_ASSET: &str = "sgt-3d-generator-runtime-0.3.0-windows-x64.exe";
const RUNTIME_URL: &str = "https://github.com/nganlinh4/screen-goated-toolbox/releases/download/sgt-runtime-bundles/sgt-3d-generator-runtime-0.3.0-windows-x64.exe";
const RUNTIME_BYTES: u64 = 1_108_992;
const RUNTIME_SHA256: &str = "16c7d45cb05d3917d4ab5eb164e7dc92eb471d8b6984bc80b002620232116dab";

pub(crate) const DOWNLOAD_TITLE: &str = "Downloading 3D Generator engine";

pub(crate) fn runtime_bundle_dir() -> PathBuf {
    crate::paths::app_local_data_dir()
        .join("3d-generator-runtime")
        .join("bin")
}

pub(crate) fn runtime_exe_path() -> PathBuf {
    runtime_bundle_dir().join(super::runtime::RUNTIME_EXE_NAME)
}

fn validate_runtime(path: &Path) -> Result<()> {
    let metadata = std::fs::metadata(path)
        .map_err(|error| anyhow!("3D Generator engine unavailable: {error}"))?;
    if !metadata.is_file() || metadata.len() != RUNTIME_BYTES {
        bail!(
            "3D Generator engine size {} does not match expected {RUNTIME_BYTES}",
            metadata.len()
        );
    }

    let modified_ms = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|value| value.as_millis())
        .unwrap_or(0);
    static CACHE: OnceLock<Mutex<Option<(u64, u128, bool)>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(None));
    if let Some((bytes, modified, valid)) = *cache.lock().unwrap_or_else(|value| value.into_inner())
        && bytes == metadata.len()
        && modified == modified_ms
    {
        return if valid {
            Ok(())
        } else {
            bail!("3D Generator engine checksum mismatch")
        };
    }

    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 128 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let valid = format!("{:x}", hasher.finalize()) == RUNTIME_SHA256;
    *cache.lock().unwrap_or_else(|value| value.into_inner()) =
        Some((metadata.len(), modified_ms, valid));
    if !valid {
        bail!("3D Generator engine checksum mismatch");
    }
    Ok(())
}

pub(crate) fn is_runtime_installed() -> bool {
    validate_runtime(&runtime_exe_path()).is_ok()
}

pub(crate) fn remove_runtime() -> Result<()> {
    let dir = runtime_bundle_dir();
    if dir.exists() {
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

pub(crate) fn download_runtime(stop: Arc<AtomicBool>, use_badge: bool) -> Result<()> {
    use crate::overlay::auto_copy_badge::{
        NotificationType, hide_progress_notification, show_detailed_notification,
        show_error_notification, show_progress_notification,
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

    let path = runtime_exe_path();
    let partial = runtime_bundle_dir().join(format!("{RUNTIME_ASSET}.download"));
    std::fs::create_dir_all(runtime_bundle_dir())?;
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    if partial.exists() {
        std::fs::remove_file(&partial)?;
    }

    let badge = crate::overlay::auto_copy_badge::locale_text();
    let title = crate::overlay::auto_copy_badge::format_locale(
        badge.downloading_runtime_fmt,
        &[("name", "3D Generator")],
    );
    let preparing = crate::overlay::auto_copy_badge::format_locale(
        badge.preparing_runtime_fmt,
        &[("name", "3D Generator")],
    );
    if let Ok(mut state) = REALTIME_STATE.lock() {
        state.is_downloading = true;
        state.download_title = DOWNLOAD_TITLE.to_string();
        state.download_message = preparing.clone();
        state.download_progress = 0.0;
    }
    if use_badge {
        show_progress_notification(&title, &preparing, 0.0);
    }

    let result = crate::api::realtime_audio::model_loader::download_file_with_progress(
        RUNTIME_URL,
        &partial,
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
            if use_badge {
                show_progress_notification(&title, &title, progress);
            }
        },
    )
    .and_then(|()| validate_runtime(&partial))
    .and_then(|()| {
        std::fs::rename(&partial, &path)
            .map_err(|error| anyhow!("Could not install 3D Generator engine: {error}"))
    })
    .and_then(|()| validate_runtime(&path));

    if result.is_err() {
        let _ = std::fs::remove_file(&partial);
    }
    if let Ok(mut state) = REALTIME_STATE.lock() {
        state.is_downloading = false;
        state.download_progress = if result.is_ok() { 100.0 } else { 0.0 };
    }
    if use_badge {
        hide_progress_notification();
        if result.is_ok() {
            let ready = crate::overlay::auto_copy_badge::format_locale(
                badge.model_ready_fmt,
                &[("name", "3D Generator")],
            );
            let installed = crate::overlay::auto_copy_badge::format_locale(
                badge.model_installed_fmt,
                &[("name", "3D Generator engine")],
            );
            show_detailed_notification(&ready, &installed, NotificationType::Success);
        } else {
            let failed = crate::overlay::auto_copy_badge::format_locale(
                badge.model_download_failed_fmt,
                &[("name", "3D Generator engine")],
            );
            show_error_notification(&failed);
        }
    }
    result
}
