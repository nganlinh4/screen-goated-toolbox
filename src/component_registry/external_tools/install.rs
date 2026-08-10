use std::collections::HashSet;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use anyhow::{Context, Result, anyhow, bail};

use super::{
    ExternalArchiveFormat, ExternalTool, ExternalToolDelivery, ExternalToolFile, ExternalToolUse,
    MAX_COMPONENT_FILES, acquire_delivery, owned_file, receipt, recovery, staging,
    validate_exact_tree, validate_install_fast, version_root,
};
use crate::component_registry::receipt::{file_matches, is_reparse_point};

static INSTALL_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(super) fn ensure(
    tool: ExternalTool,
    delivery: &'static ExternalToolDelivery,
    cancelled: &AtomicBool,
    on_progress: impl Fn(u64, u64),
) -> Result<ExternalToolUse> {
    let failure_reason = match acquire_delivery(tool, delivery) {
        Ok(component) => return Ok(component),
        Err(error) => format!("{error:#}"),
    };
    let _recovery = recovery::quarantine_invalid(delivery, &failure_reason)?;
    let _install_lease = crate::component_registry::acquire(delivery.id)?;
    if adopt_legacy(delivery)? {
        return acquire_delivery(tool, delivery);
    }
    install_delivery(delivery, cancelled, on_progress)?;
    acquire_delivery(tool, delivery)
}

fn install_delivery(
    delivery: &ExternalToolDelivery,
    cancelled: &AtomicBool,
    on_progress: impl Fn(u64, u64),
) -> Result<()> {
    let sequence = INSTALL_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let working = crate::paths::app_runtime_local_data_dir();
    let downloads = working.join("component-downloads");
    staging::ensure_directory_tree(&working, &downloads)?;
    let archive = downloads.join(format!(
        "{}-{}-{sequence}.download",
        delivery.id,
        std::process::id()
    ));
    let staging_parent = working.join("component-staging");
    staging::ensure_directory_tree(&working, &staging_parent)?;
    let stage = staging_parent.join(format!(
        "{}-{}-{}-{sequence}",
        delivery.id,
        delivery.version,
        std::process::id()
    ));
    std::fs::create_dir(&stage)?;

    let result = (|| {
        download(delivery, &archive, cancelled, on_progress)?;
        validate_archive(delivery, &archive)?;
        extract(delivery, &archive, &stage, cancelled)?;
        validate_staging(delivery, &stage)?;
        sync_staged_files(delivery, &stage)?;
        crate::component_registry::write_receipt(&stage, &receipt(delivery))?;
        finish_staging(delivery, &stage)
    })();
    let _ = std::fs::remove_file(&archive);
    if stage.exists() {
        let owned = cleanup_paths(delivery, &stage);
        let _ = staging::cleanup_owned(&stage, &owned);
    }
    result
}

fn download(
    delivery: &ExternalToolDelivery,
    target: &Path,
    cancelled: &AtomicBool,
    on_progress: impl Fn(u64, u64),
) -> Result<()> {
    let response = crate::api::client::UREQ_DOWNLOAD_AGENT
        .get(delivery.download_url)
        .header("User-Agent", "ScreenGoatedToolbox")
        .call()
        .with_context(|| format!("{} download failed", delivery.id))?;
    if response
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .is_some_and(|size| size != delivery.size_bytes)
    {
        bail!("{} download size does not match this build", delivery.id);
    }
    let mut reader = response.into_body().into_reader();
    let mut output = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(target)?;
    copy_bounded(
        &mut reader,
        &mut output,
        delivery.size_bytes,
        cancelled,
        on_progress,
    )?;
    output.flush()?;
    output.sync_all()?;
    Ok(())
}

fn copy_bounded(
    reader: &mut impl Read,
    writer: &mut impl Write,
    exact_size: u64,
    cancelled: &AtomicBool,
    on_progress: impl Fn(u64, u64),
) -> Result<()> {
    let mut buffer = [0_u8; 128 * 1024];
    let mut copied = 0_u64;
    loop {
        if cancelled.load(Ordering::Relaxed) {
            bail!("external tool download was cancelled");
        }
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        copied = copied
            .checked_add(read as u64)
            .ok_or_else(|| anyhow!("external tool artifact is too large"))?;
        if copied > exact_size {
            bail!("external tool artifact exceeds its exact size");
        }
        writer.write_all(&buffer[..read])?;
        on_progress(copied, exact_size);
    }
    if copied != exact_size {
        bail!("external tool artifact is incomplete");
    }
    Ok(())
}

fn validate_archive(delivery: &ExternalToolDelivery, path: &Path) -> Result<()> {
    let expected = crate::component_registry::OwnedComponentFile {
        path: delivery.asset.into(),
        size_bytes: delivery.size_bytes,
        sha256: delivery.sha256.to_string(),
    };
    if !file_matches(path, &expected)? {
        bail!("{} artifact checksum mismatch", delivery.id);
    }
    Ok(())
}

fn extract(
    delivery: &ExternalToolDelivery,
    archive: &Path,
    stage: &Path,
    cancelled: &AtomicBool,
) -> Result<()> {
    match delivery.archive_format {
        ExternalArchiveFormat::Raw => extract_raw(delivery, archive, stage, cancelled),
        ExternalArchiveFormat::Zip => extract_zip(delivery, archive, stage, cancelled),
    }
}

fn extract_raw(
    delivery: &ExternalToolDelivery,
    archive: &Path,
    stage: &Path,
    cancelled: &AtomicBool,
) -> Result<()> {
    if delivery.files.len() != 1 || delivery.files[0].size_bytes != delivery.size_bytes {
        bail!("raw external tool manifest is inconsistent");
    }
    let expected = &delivery.files[0];
    let target = staging::prepare_target(stage, Path::new(expected.path))?;
    let mut input = std::fs::File::open(archive)?;
    let mut output = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&target)?;
    copy_bounded(
        &mut input,
        &mut output,
        expected.size_bytes,
        cancelled,
        |_, _| {},
    )?;
    output.flush()?;
    output.sync_all()?;
    validate_extracted(&target, expected)
}

fn extract_zip(
    delivery: &ExternalToolDelivery,
    archive_path: &Path,
    stage: &Path,
    cancelled: &AtomicBool,
) -> Result<()> {
    let file = std::fs::File::open(archive_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    if archive.len() != delivery.files.len() || archive.len() > MAX_COMPONENT_FILES {
        bail!("external tool ZIP has an unexpected entry count");
    }
    let mut extracted = HashSet::new();
    let mut expanded = 0_u64;
    for index in 0..archive.len() {
        if cancelled.load(Ordering::Relaxed) {
            bail!("external tool extraction was cancelled");
        }
        let mut entry = archive.by_index(index)?;
        let relative = entry
            .enclosed_name()
            .ok_or_else(|| anyhow!("external tool ZIP contains an unsafe path"))?
            .to_path_buf();
        reject_non_file_zip_entry(&entry)?;
        let expected = delivery
            .files
            .iter()
            .find(|file| Path::new(file.archive_path) == relative)
            .ok_or_else(|| anyhow!("external tool ZIP contains an undeclared file"))?;
        if !extracted.insert(relative) || entry.size() != expected.size_bytes {
            bail!("external tool ZIP entry does not match its manifest");
        }
        expanded = expanded
            .checked_add(entry.size())
            .ok_or_else(|| anyhow!("external tool ZIP expands beyond its limit"))?;
        if expanded > delivery.unpacked_size_bytes {
            bail!("external tool ZIP expands beyond its declared size");
        }
        let target = staging::prepare_target(stage, Path::new(expected.path))?;
        let mut output = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&target)?;
        copy_bounded(
            &mut entry,
            &mut output,
            expected.size_bytes,
            cancelled,
            |_, _| {},
        )?;
        output.flush()?;
        output.sync_all()?;
        validate_extracted(&target, expected)?;
    }
    if extracted.len() != delivery.files.len() || expanded != delivery.unpacked_size_bytes {
        bail!("external tool ZIP is incomplete");
    }
    Ok(())
}

fn reject_non_file_zip_entry(entry: &zip::read::ZipFile<'_, std::fs::File>) -> Result<()> {
    if entry.is_dir() {
        bail!("external tool ZIP contains a directory entry");
    }
    if let Some(mode) = entry.unix_mode() {
        let kind = mode & 0o170000;
        if kind != 0 && kind != 0o100000 {
            bail!("external tool ZIP contains a link or special file");
        }
    }
    Ok(())
}

fn validate_extracted(path: &Path, expected: &ExternalToolFile) -> Result<()> {
    if !file_matches(path, &owned_file(expected))? {
        bail!("extracted external tool failed integrity verification");
    }
    if path
        .extension()
        .is_some_and(|extension| extension.to_string_lossy().eq_ignore_ascii_case("exe"))
    {
        validate_x64_pe(path)?;
    }
    Ok(())
}

fn validate_staging(delivery: &ExternalToolDelivery, stage: &Path) -> Result<()> {
    for file in delivery.files {
        validate_extracted(&stage.join(file.path), file)?;
    }
    validate_exact_tree(stage, delivery.files)
}

fn sync_staged_files(delivery: &ExternalToolDelivery, stage: &Path) -> Result<()> {
    for file in delivery.files {
        std::fs::File::open(stage.join(file.path))?.sync_all()?;
    }
    Ok(())
}

fn finish_staging(delivery: &ExternalToolDelivery, stage: &Path) -> Result<()> {
    let parent = crate::component_registry::ensure_component_parent(delivery.id)?;
    let target = version_root(delivery)?;
    if target.parent() != Some(parent.as_path()) || target.exists() {
        bail!("external tool install target is invalid");
    }
    std::fs::rename(stage, &target)?;
    validate_install_fast(delivery)
}

fn legacy_bin_dir() -> PathBuf {
    crate::paths::app_local_data_dir().join("bin")
}

fn adopt_legacy(delivery: &ExternalToolDelivery) -> Result<bool> {
    let legacy = legacy_bin_dir();
    if !legacy.is_dir() {
        return Ok(false);
    }
    require_regular_directory(&legacy)?;
    if delivery.files.iter().any(|file| {
        Path::new(file.path)
            .strip_prefix("bin/x64")
            .ok()
            .and_then(Path::file_name)
            .is_none()
    }) {
        return Ok(false);
    }
    let sequence = INSTALL_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let parent = crate::paths::app_runtime_local_data_dir().join("component-staging");
    staging::ensure_directory_tree(&parent, &parent)?;
    let stage = parent.join(format!(
        "{}-adopt-{}-{sequence}",
        delivery.id,
        std::process::id()
    ));
    std::fs::create_dir(&stage)?;
    let result = adopt_from(&legacy, &stage, delivery);
    match result {
        Ok(true) => {
            sync_staged_files(delivery, &stage)?;
            crate::component_registry::write_receipt(&stage, &receipt(delivery))?;
            finish_staging(delivery, &stage)?;
            cleanup_verified_legacy(&legacy, delivery);
            Ok(true)
        }
        Ok(false) => {
            let _ = staging::cleanup_owned(&stage, &cleanup_paths(delivery, &stage));
            Ok(false)
        }
        Err(error) => {
            let _ = staging::cleanup_owned(&stage, &cleanup_paths(delivery, &stage));
            Err(error)
        }
    }
}

fn adopt_from(legacy: &Path, stage: &Path, delivery: &ExternalToolDelivery) -> Result<bool> {
    for file in delivery.files {
        let name = Path::new(file.path).file_name().unwrap();
        let source = legacy.join(name);
        if !file_matches(&source, &owned_file(file)).unwrap_or(false) {
            return Ok(false);
        }
        validate_x64_pe(&source)?;
    }
    for file in delivery.files {
        let source = legacy.join(Path::new(file.path).file_name().unwrap());
        let target = staging::prepare_target(stage, Path::new(file.path))?;
        std::fs::hard_link(&source, &target).or_else(|_| {
            std::fs::copy(&source, &target)?;
            Ok::<(), std::io::Error>(())
        })?;
        validate_extracted(&target, file)?;
    }
    Ok(true)
}

fn cleanup_verified_legacy(legacy: &Path, delivery: &ExternalToolDelivery) {
    for file in delivery.files {
        let Some(name) = Path::new(file.path).file_name() else {
            continue;
        };
        let path = legacy.join(name);
        if file_matches(&path, &owned_file(file)).unwrap_or(false) && validate_x64_pe(&path).is_ok()
        {
            let _ = std::fs::remove_file(path);
        }
    }
}

pub(super) fn reconcile_interrupted() -> Result<()> {
    let working = crate::paths::app_runtime_local_data_dir();
    cleanup_interrupted_staging(&working.join("component-staging"))?;
    cleanup_interrupted_downloads(&working.join("component-downloads"))
}

fn cleanup_interrupted_staging(parent: &Path) -> Result<()> {
    let Ok(metadata) = std::fs::symlink_metadata(parent) else {
        return Ok(());
    };
    if !metadata.is_dir() || is_reparse_point(&metadata) {
        bail!("external tool staging parent is unsafe");
    }
    for entry in std::fs::read_dir(parent)?.take(64) {
        let path = entry?.path();
        let metadata = std::fs::symlink_metadata(&path)?;
        if !metadata.is_dir() || is_reparse_point(&metadata) {
            continue;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Some(delivery) = super::EXTERNAL_TOOL_DELIVERIES
            .iter()
            .find(|delivery| name.starts_with(&format!("{}-", delivery.id)))
        else {
            continue;
        };
        let owned = cleanup_paths(delivery, &path);
        let _ = staging::cleanup_owned(&path, &owned);
    }
    Ok(())
}

fn cleanup_paths(delivery: &ExternalToolDelivery, stage: &Path) -> Vec<PathBuf> {
    let mut owned = delivery
        .files
        .iter()
        .map(|file| PathBuf::from(file.path))
        .collect::<Vec<_>>();
    let receipt_path = stage.join(crate::component_registry::receipt::RECEIPT_NAME);
    if crate::component_registry::ComponentReceipt::read(&receipt_path)
        .is_ok_and(|receipt| receipt.id == delivery.id && receipt.version == delivery.version)
    {
        owned.push(crate::component_registry::receipt::RECEIPT_NAME.into());
    }
    owned
}

fn cleanup_interrupted_downloads(parent: &Path) -> Result<()> {
    let Ok(metadata) = std::fs::symlink_metadata(parent) else {
        return Ok(());
    };
    if !metadata.is_dir() || is_reparse_point(&metadata) {
        bail!("external tool download parent is unsafe");
    }
    for entry in std::fs::read_dir(parent)?.take(64) {
        let path = entry?.path();
        let metadata = std::fs::symlink_metadata(&path)?;
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if metadata.is_file()
            && !is_reparse_point(&metadata)
            && super::EXTERNAL_TOOL_DELIVERIES
                .iter()
                .any(|delivery| generated_download_name(name, delivery.id))
        {
            let _ = std::fs::remove_file(path);
        }
    }
    Ok(())
}

fn generated_download_name(name: &str, id: &str) -> bool {
    let Some(body) = name
        .strip_prefix(&format!("{id}-"))
        .and_then(|value| value.strip_suffix(".download"))
    else {
        return false;
    };
    let mut pieces = body.split('-');
    matches!((pieces.next(), pieces.next(), pieces.next()), (Some(pid), Some(sequence), None) if pid.bytes().all(|byte| byte.is_ascii_digit()) && sequence.bytes().all(|byte| byte.is_ascii_digit()))
}

fn validate_x64_pe(path: &Path) -> Result<()> {
    use std::io::{Seek as _, SeekFrom};
    let mut file = std::fs::File::open(path)?;
    let mut dos = [0_u8; 64];
    file.read_exact(&mut dos)?;
    if &dos[..2] != b"MZ" {
        bail!("external tool executable is not PE");
    }
    let offset = u32::from_le_bytes(dos[0x3c..0x40].try_into().unwrap()) as u64;
    if offset > 1024 * 1024 {
        bail!("external tool executable has an invalid PE offset");
    }
    file.seek(SeekFrom::Start(offset))?;
    let mut header = [0_u8; 6];
    file.read_exact(&mut header)?;
    if &header[..4] != b"PE\0\0" || u16::from_le_bytes([header[4], header[5]]) != 0x8664 {
        bail!("external tool executable is not Windows x64");
    }
    Ok(())
}

fn require_regular_directory(path: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_dir() || is_reparse_point(&metadata) {
        bail!("legacy external tool directory is unsafe");
    }
    Ok(())
}

#[cfg(test)]
pub(super) fn adopt_from_for_test(
    legacy: &Path,
    stage: &Path,
    delivery: &ExternalToolDelivery,
) -> Result<bool> {
    adopt_from(legacy, stage, delivery)
}

#[cfg(test)]
pub(super) fn generated_download_name_for_test(name: &str, id: &str) -> bool {
    generated_download_name(name, id)
}

#[cfg(test)]
pub(super) fn extract_zip_for_test(
    delivery: &ExternalToolDelivery,
    archive: &Path,
    stage: &Path,
) -> Result<()> {
    extract_zip(delivery, archive, stage, &AtomicBool::new(false))
}

#[cfg(test)]
pub(super) fn quarantine_invalid_for_test(delivery: &ExternalToolDelivery) -> Result<PathBuf> {
    recovery::quarantine_invalid(delivery, "test integrity failure")?
        .ok_or_else(|| anyhow!("test install was not quarantined"))
}

#[cfg(test)]
pub(super) fn finish_staging_for_test(delivery: &ExternalToolDelivery, stage: &Path) -> Result<()> {
    finish_staging(delivery, stage)
}
