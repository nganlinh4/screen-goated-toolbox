use std::collections::HashSet;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};

use super::super::staging;
use super::{
    EngineDelivery, ID, MAX_COMPONENT_FILES, owned_file, receipt, validate_exact_tree,
    validate_install, version_root,
};
use crate::component_registry::RemovalOutcome;
use crate::component_registry::receipt::{file_matches, is_reparse_point};

static INSTALL_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));
static INSTALL_SEQUENCE: AtomicU64 = AtomicU64::new(0);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(10 * 60);

pub(super) fn ensure(
    delivery: &'static EngineDelivery,
    cancelled: &AtomicBool,
    on_progress: impl Fn(u64, u64),
) -> Result<()> {
    let _guard = INSTALL_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if validate_install(delivery).is_ok() {
        return Ok(());
    }
    clear_invalid_install()?;
    let _install_lease = crate::component_registry::acquire(ID)?;
    install_delivery(delivery, cancelled, on_progress)
}

fn clear_invalid_install() -> Result<()> {
    match crate::component_registry::request_remove(ID)? {
        RemovalOutcome::Missing | RemovalOutcome::Removed => Ok(()),
        RemovalOutcome::Pending => bail!("{ID} is currently in use"),
        RemovalOutcome::PreservedModified(paths) => bail!(
            "{ID} cannot be repaired because {} modified managed file(s) were preserved",
            paths.len()
        ),
        RemovalOutcome::RequiredBy(dependents) => {
            bail!("{ID} repair is blocked by {}", dependents.join(", "))
        }
    }
}

fn install_delivery(
    delivery: &EngineDelivery,
    cancelled: &AtomicBool,
    on_progress: impl Fn(u64, u64),
) -> Result<()> {
    let sequence = INSTALL_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let app_data = crate::paths::app_local_data_dir();
    let scratch = app_data.join("component-downloads");
    staging::ensure_directory_tree(&app_data, &scratch)?;
    let archive = scratch.join(format!("{ID}-{}-{sequence}.download", std::process::id()));
    let staging_parent = app_data.join("component-staging");
    staging::ensure_directory_tree(&app_data, &staging_parent)?;
    let stage = staging_parent.join(format!(
        "{ID}-{}-{}-{sequence}",
        delivery.version,
        std::process::id()
    ));
    std::fs::create_dir(&stage)?;

    let result = (|| {
        download(delivery, &archive, cancelled, on_progress)?;
        validate_archive(delivery, &archive)?;
        extract(delivery, &archive, &stage)?;
        validate_staging(delivery, &stage)?;
        crate::component_registry::write_receipt(&stage, &receipt(delivery))?;
        let parent = crate::component_registry::ensure_component_parent(ID)?;
        let target = version_root(delivery)?;
        if target.parent() != Some(parent.as_path()) || target.exists() {
            bail!("Computer Control engine install target is invalid");
        }
        std::fs::rename(&stage, &target)?;
        validate_install(delivery)
    })();
    let _ = std::fs::remove_file(&archive);
    if stage.exists() {
        let owned = delivery
            .files
            .iter()
            .map(|file| PathBuf::from(file.path))
            .collect::<Vec<_>>();
        let _ = staging::cleanup_owned(&stage, &owned);
    }
    result
}

fn download(
    delivery: &EngineDelivery,
    target: &Path,
    cancelled: &AtomicBool,
    on_progress: impl Fn(u64, u64),
) -> Result<()> {
    let request = crate::api::client::UREQ_DOWNLOAD_AGENT
        .get(delivery.download_url)
        .config()
        .https_only(true)
        .timeout_global(Some(DOWNLOAD_TIMEOUT))
        .build();
    let response = request
        .header("User-Agent", "ScreenGoatedToolbox")
        .call()
        .context("Computer Control engine download failed")?;
    if response
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .is_some_and(|size| size != delivery.size_bytes)
    {
        bail!("Computer Control engine download size does not match this build");
    }
    let mut reader = response.into_body().into_reader();
    let mut output = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(target)?;
    let mut buffer = [0_u8; 128 * 1024];
    let mut downloaded = 0_u64;
    loop {
        if cancelled.load(Ordering::Relaxed) {
            bail!("Computer Control engine download was cancelled");
        }
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        downloaded = downloaded
            .checked_add(read as u64)
            .ok_or_else(|| anyhow!("Computer Control engine download is too large"))?;
        if downloaded > delivery.size_bytes {
            bail!("Computer Control engine download exceeds its exact size");
        }
        output.write_all(&buffer[..read])?;
        on_progress(downloaded, delivery.size_bytes);
    }
    if downloaded != delivery.size_bytes {
        bail!("Computer Control engine download is incomplete");
    }
    output.flush()?;
    output.sync_all()?;
    Ok(())
}

fn validate_archive(delivery: &EngineDelivery, path: &Path) -> Result<()> {
    let expected = crate::component_registry::OwnedComponentFile {
        path: delivery.asset.into(),
        size_bytes: delivery.size_bytes,
        sha256: delivery.sha256.to_string(),
    };
    if !file_matches(path, &expected)? {
        bail!("Computer Control engine archive checksum mismatch");
    }
    Ok(())
}

fn extract(delivery: &EngineDelivery, archive_path: &Path, stage: &Path) -> Result<()> {
    let file = std::fs::File::open(archive_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    if archive.len() != delivery.files.len() || archive.len() > MAX_COMPONENT_FILES {
        bail!("Computer Control engine archive has an unexpected entry count");
    }
    let mut extracted = HashSet::new();
    let mut extracted_bytes = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        if entry.is_dir() {
            bail!("Computer Control engine archive contains a directory entry");
        }
        let relative = entry
            .enclosed_name()
            .ok_or_else(|| anyhow!("Computer Control engine archive path is unsafe"))?
            .to_path_buf();
        let expected = delivery
            .files
            .iter()
            .find(|file| Path::new(file.path) == relative)
            .ok_or_else(|| anyhow!("Computer Control archive contains an unowned file"))?;
        if !extracted.insert(relative.clone()) || entry.size() != expected.size_bytes {
            bail!("Computer Control engine archive entry does not match its manifest");
        }
        extracted_bytes = extracted_bytes
            .checked_add(entry.size())
            .ok_or_else(|| anyhow!("Computer Control engine expands beyond its limit"))?;
        if extracted_bytes > delivery.unpacked_size_bytes {
            bail!("Computer Control engine expands beyond its declared size");
        }
        let target = staging::prepare_target(stage, &relative)?;
        let mut output = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&target)?;
        std::io::copy(&mut entry, &mut output)?;
        output.flush()?;
        if !file_matches(&target, &owned_file(expected))? {
            bail!("Computer Control engine extracted checksum mismatch");
        }
    }
    if extracted.len() != delivery.files.len() || extracted_bytes != delivery.unpacked_size_bytes {
        bail!("Computer Control engine archive is incomplete");
    }
    Ok(())
}

fn validate_staging(delivery: &EngineDelivery, stage: &Path) -> Result<()> {
    for file in delivery.files {
        if !file_matches(&stage.join(file.path), &owned_file(file))? {
            bail!("staged Computer Control engine failed integrity verification");
        }
    }
    validate_exact_tree(stage, delivery.files, false)?;
    if std::fs::symlink_metadata(stage)
        .map(|metadata| is_reparse_point(&metadata))
        .unwrap_or(true)
    {
        bail!("staged Computer Control engine root is unsafe");
    }
    Ok(())
}
