use std::collections::HashSet;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use anyhow::{Result, anyhow, bail};

use super::*;
use crate::component_registry::receipt::ComponentReceipt;

static INSTALL_SEQUENCE: AtomicU64 = AtomicU64::new(0);
static DOWNLOAD_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

pub(super) fn download(stop: Arc<AtomicBool>, use_badge: bool) -> Result<()> {
    let _download = DOWNLOAD_LOCK
        .lock()
        .unwrap_or_else(|value| value.into_inner());
    if is_installed() {
        return Ok(());
    }
    let delivery =
        delivery().ok_or_else(|| anyhow!("Creation download contract is unavailable"))?;
    let name = localized_name();
    let badge =
        use_badge.then(|| crate::overlay::auto_copy_badge::DownloadProgressBadge::new(&name));
    set_progress(0.0, &name);
    let result = {
        let _mutation = super::super::acquire_mutation_guard()?;
        install_delivery(delivery, &stop, |downloaded, total| {
            set_progress(downloaded as f32 / total as f32 * 100.0, &name);
            if let Some(badge) = &badge {
                badge.report(downloaded, total)
            }
        })
    };
    finish_progress(result.is_ok());
    if let Some(badge) = &badge {
        badge.finish()
    }
    if result.is_ok() {
        if use_badge {
            notify_success(&name)
        }
        if let Err(error) = remove_legacy_components() {
            crate::log_info!("[Creation] Legacy payload cleanup deferred: {error:#}");
        }
    }
    result
}

fn clear_previous_install() -> Result<()> {
    match super::super::request_remove_and_wait(COMPONENT_ID)? {
        RemovalOutcome::Missing | RemovalOutcome::Removed => Ok(()),
        RemovalOutcome::Pending => bail!("Creation update is waiting for active use to end"),
        RemovalOutcome::RequiredBy(dependents) => {
            bail!("Creation repair is blocked by: {}", dependents.join(", "))
        }
        RemovalOutcome::PreservedModified(paths) => {
            bail!("Creation repair preserved {} unsafe path(s)", paths.len())
        }
    }
}

fn install_delivery(
    delivery: &CreationDelivery,
    stop: &AtomicBool,
    on_progress: impl Fn(u64, u64),
) -> Result<()> {
    let sequence = INSTALL_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let scratch = crate::paths::app_local_data_dir().join("component-downloads");
    ensure_directory(&scratch)?;
    let archive = scratch.join(format!(
        "{COMPONENT_ID}-{}-{sequence}.download",
        std::process::id()
    ));
    let staging_parent = crate::paths::app_local_data_dir().join("component-staging");
    ensure_directory(&staging_parent)?;
    let staging = staging_parent.join(format!(
        "{COMPONENT_ID}-{}-{}-{sequence}",
        delivery.version,
        std::process::id()
    ));
    std::fs::create_dir(&staging)?;
    let result = (|| {
        download_archive(delivery, &archive, stop, on_progress)?;
        validate_archive(&archive, delivery)?;
        extract_archive(&archive, &staging, delivery)?;
        super::super::write_receipt(&staging, &receipt(delivery))?;
        validation::validate_exact_tree(&staging, delivery)?;
        clear_previous_install()?;
        let target = version_root(delivery)?;
        let parent = super::super::ensure_component_parent(COMPONENT_ID)?;
        if target.parent() != Some(parent.as_path()) || target.exists() {
            bail!("Creation install target is unavailable");
        }
        std::fs::rename(&staging, &target)?;
        validation::validate_install(delivery)
    })();
    let _ = std::fs::remove_file(&archive);
    if staging.exists() {
        let paths = delivery
            .files
            .iter()
            .map(|file| file.path)
            .collect::<Vec<_>>();
        let _ = staging::cleanup_owned(&staging, &paths);
    }
    result
}

fn download_archive(
    delivery: &CreationDelivery,
    target: &Path,
    stop: &AtomicBool,
    on_progress: impl Fn(u64, u64),
) -> Result<()> {
    let response = crate::api::client::UREQ_DOWNLOAD_AGENT
        .get(delivery.download_url)
        .header("User-Agent", "ScreenGoatedToolbox")
        .call()
        .map_err(|error| anyhow!("Creation download failed: {error}"))?;
    let declared = response
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());
    if declared.is_some_and(|size| size != delivery.size_bytes) {
        bail!("Creation download size does not match this build")
    }
    let mut reader = response.into_body().into_reader();
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)?;
    let mut buffer = [0_u8; 128 * 1024];
    let mut downloaded = 0_u64;
    loop {
        if stop.load(Ordering::Relaxed) {
            bail!("Creation download was cancelled")
        }
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        downloaded = downloaded
            .checked_add(read as u64)
            .ok_or_else(|| anyhow!("Creation download is too large"))?;
        if downloaded > delivery.size_bytes {
            bail!("Creation download exceeds its contract")
        }
        output.write_all(&buffer[..read])?;
        on_progress(downloaded, delivery.size_bytes);
    }
    if downloaded != delivery.size_bytes {
        bail!("Creation download is incomplete")
    }
    output.flush()?;
    output.sync_all()?;
    Ok(())
}

fn validate_archive(path: &Path, delivery: &CreationDelivery) -> Result<()> {
    let expected = OwnedComponentFile {
        path: PathBuf::from(delivery.asset),
        size_bytes: delivery.size_bytes,
        sha256: delivery.sha256.to_string(),
    };
    if !file_matches(path, &expected)? {
        bail!("Creation archive checksum mismatch")
    }
    Ok(())
}

fn extract_archive(path: &Path, staging: &Path, delivery: &CreationDelivery) -> Result<()> {
    let mut archive = zip::ZipArchive::new(std::fs::File::open(path)?)?;
    if archive.len() != delivery.files.len() || archive.len() > MAX_ARCHIVE_ENTRIES {
        bail!("Creation archive entry count is invalid")
    }
    let mut extracted = HashSet::new();
    let mut extracted_bytes = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        if entry.is_dir() {
            bail!("Creation archive contains a directory entry")
        }
        let relative = entry
            .enclosed_name()
            .ok_or_else(|| anyhow!("Creation archive path is unsafe"))?
            .to_path_buf();
        validate_relative_path(&relative)?;
        let expected = delivery
            .files
            .iter()
            .find(|file| Path::new(file.path) == relative)
            .ok_or_else(|| anyhow!("Creation archive contains an unowned file"))?;
        if !extracted.insert(relative.clone()) || entry.size() != expected.size_bytes {
            bail!("Creation archive entry does not match its contract")
        }
        extracted_bytes = extracted_bytes
            .checked_add(entry.size())
            .ok_or_else(|| anyhow!("Creation archive expands too large"))?;
        if extracted_bytes > delivery.unpacked_size_bytes {
            bail!("Creation archive expands beyond its contract")
        }
        let target = staging::prepare_target(staging, &relative)?;
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)?;
        std::io::copy(&mut entry, &mut output)?;
        output.flush()?;
        if !file_matches(&target, &owned_file(expected))? {
            bail!("Creation archive entry checksum mismatch")
        }
    }
    if extracted.len() != delivery.files.len() || extracted_bytes != delivery.unpacked_size_bytes {
        bail!("Creation archive is incomplete")
    }
    Ok(())
}

fn receipt(delivery: &CreationDelivery) -> ComponentReceipt {
    ComponentReceipt {
        schema_version: 1,
        id: COMPONENT_ID.into(),
        version: delivery.version.into(),
        architecture: ARCHITECTURE.into(),
        dependencies: Vec::new(),
        files: delivery.files.iter().map(owned_file).collect(),
    }
}

fn ensure_directory(path: &Path) -> Result<()> {
    std::fs::create_dir_all(path)?;
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_dir() || super::super::receipt::is_reparse_point(&metadata) {
        bail!("Creation working directory is unsafe")
    }
    Ok(())
}

fn set_progress(progress: f32, name: &str) {
    if let Ok(mut state) = crate::overlay::realtime_webview::state::REALTIME_STATE.lock() {
        state.is_downloading = true;
        state.download_title = name.to_string();
        state.download_message = name.to_string();
        state.download_progress = progress;
    }
}

fn finish_progress(success: bool) {
    if let Ok(mut state) = crate::overlay::realtime_webview::state::REALTIME_STATE.lock() {
        state.is_downloading = false;
        state.download_progress = if success { 100.0 } else { 0.0 };
    }
}

fn notify_success(name: &str) {
    let locale = crate::overlay::auto_copy_badge::locale_text();
    let title = crate::overlay::auto_copy_badge::format_locale(
        locale.component_installed_fmt,
        &[("name", name)],
    );
    crate::overlay::auto_copy_badge::show_detailed_notification(
        &title,
        name,
        crate::overlay::auto_copy_badge::NotificationType::Success,
    );
}
