use std::collections::HashSet;
use std::io::{Read as _, Seek as _, SeekFrom, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};

use super::{
    DetectorDelivery, ID, MAX_COMPONENT_FILES, receipt, validate_exact_tree, validate_install,
    version_root,
};
use crate::component_registry::RemovalOutcome;
use crate::component_registry::receipt::{file_matches, is_reparse_point};
use crate::component_registry::staging;

static INSTALL_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));
static INSTALL_SEQUENCE: AtomicU64 = AtomicU64::new(0);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(10 * 60);

pub(super) fn ensure(
    delivery: &'static DetectorDelivery,
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
    delivery: &DetectorDelivery,
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
            bail!("Screen Translate detector install target is invalid");
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
    delivery: &DetectorDelivery,
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
        .context("Screen Translate detector download failed")?;
    if response
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .is_some_and(|size| size != delivery.size_bytes)
    {
        bail!("Screen Translate detector download size does not match this build");
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
            bail!("Screen Translate detector download was cancelled");
        }
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        downloaded = downloaded
            .checked_add(read as u64)
            .filter(|size| *size <= delivery.size_bytes)
            .ok_or_else(|| anyhow!("Screen Translate detector download exceeds its size"))?;
        output.write_all(&buffer[..read])?;
        on_progress(downloaded, delivery.size_bytes);
    }
    if downloaded != delivery.size_bytes {
        bail!("Screen Translate detector download is incomplete");
    }
    output.flush()?;
    output.sync_all()?;
    Ok(())
}

fn validate_archive(delivery: &DetectorDelivery, path: &Path) -> Result<()> {
    let expected = crate::component_registry::OwnedComponentFile {
        path: delivery.asset.into(),
        size_bytes: delivery.size_bytes,
        sha256: delivery.sha256.to_string(),
    };
    if !file_matches(path, &expected)? {
        bail!("Screen Translate detector archive checksum mismatch");
    }
    Ok(())
}

fn extract(delivery: &DetectorDelivery, archive_path: &Path, stage: &Path) -> Result<()> {
    let mut archive = zip::ZipArchive::new(std::fs::File::open(archive_path)?)?;
    if archive.len() != delivery.files.len() || archive.len() > MAX_COMPONENT_FILES {
        bail!("Screen Translate detector archive has an unexpected entry count");
    }
    let mut extracted = HashSet::new();
    let mut extracted_bytes = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        if entry.is_dir() {
            bail!("Screen Translate detector archive contains a directory entry");
        }
        let relative = entry
            .enclosed_name()
            .ok_or_else(|| anyhow!("Screen Translate detector archive path is unsafe"))?
            .to_path_buf();
        let expected = delivery
            .files
            .iter()
            .find(|file| Path::new(file.path) == relative)
            .ok_or_else(|| anyhow!("Screen Translate detector archive has an unowned file"))?;
        if !extracted.insert(relative.clone()) || entry.size() != expected.size_bytes {
            bail!("Screen Translate detector archive entry does not match its manifest");
        }
        extracted_bytes = extracted_bytes
            .checked_add(entry.size())
            .filter(|size| *size <= delivery.unpacked_size_bytes)
            .ok_or_else(|| anyhow!("Screen Translate detector expands beyond its size"))?;
        let target = staging::prepare_target(stage, &relative)?;
        let mut output = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&target)?;
        std::io::copy(&mut entry, &mut output)?;
        output.flush()?;
        if !file_matches(&target, &super::owned_file(expected))? {
            bail!("Screen Translate detector extracted checksum mismatch");
        }
    }
    if extracted.len() != delivery.files.len() || extracted_bytes != delivery.unpacked_size_bytes {
        bail!("Screen Translate detector archive is incomplete");
    }
    Ok(())
}

fn validate_staging(delivery: &DetectorDelivery, stage: &Path) -> Result<()> {
    for file in delivery.files {
        if !file_matches(&stage.join(file.path), &super::owned_file(file))? {
            bail!("staged Screen Translate detector failed integrity verification");
        }
    }
    validate_exact_tree(stage, delivery.files, false)?;
    if std::fs::symlink_metadata(stage)
        .map(|metadata| is_reparse_point(&metadata))
        .unwrap_or(true)
    {
        bail!("staged Screen Translate detector root is unsafe");
    }
    validate_x64_pe(&stage.join(super::EXECUTABLE_PATH))
}

pub(super) fn validate_x64_pe(path: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_file() || is_reparse_point(&metadata) {
        bail!("Screen Translate detector worker is unsafe");
    }
    let mut file = std::fs::File::open(path)?;
    let mut dos = [0_u8; 64];
    file.read_exact(&mut dos)?;
    if &dos[..2] != b"MZ" {
        bail!("Screen Translate detector worker is not a PE executable");
    }
    let offset = u32::from_le_bytes(dos[0x3c..0x40].try_into().expect("PE offset"));
    file.seek(SeekFrom::Start(u64::from(offset)))?;
    let mut header = [0_u8; 6];
    file.read_exact(&mut header)?;
    if &header[..4] != b"PE\0\0" || u16::from_le_bytes([header[4], header[5]]) != 0x8664 {
        bail!("Screen Translate detector worker is not an x64 PE executable");
    }
    Ok(())
}
