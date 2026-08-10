use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

use anyhow::{Result, anyhow, bail};

use super::super::receipt::{ComponentReceipt, OwnedComponentFile, file_matches};
use super::{
    ARCHITECTURE, COMPONENT_ID, QwenRuntimeArchive, QwenRuntimeDelivery, VC_COMPONENT_ID, archive,
    delivery, owned_file, validate_exact_tree, validate_install, version_root,
};

static INSTALL_SEQUENCE: AtomicU64 = AtomicU64::new(0);
static INSTALL_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

pub(super) fn install(cancel: &AtomicBool, on_progress: impl Fn(u64, u64)) -> Result<()> {
    let _guard = INSTALL_LOCK
        .lock()
        .unwrap_or_else(|value| value.into_inner());
    let delivery = delivery()?;
    if validate_install(delivery).is_ok() {
        return Ok(());
    }
    clear_invalid_install()?;
    let _install_lease = super::super::acquire(COMPONENT_ID)?;
    if adopt_legacy(delivery, cancel, &on_progress)? {
        return Ok(());
    }
    install_delivery(delivery, cancel, &on_progress)
}

fn clear_invalid_install() -> Result<()> {
    match super::super::request_remove(COMPONENT_ID)? {
        super::super::RemovalOutcome::Missing | super::super::RemovalOutcome::Removed => Ok(()),
        super::super::RemovalOutcome::Pending => bail!("Qwen3 runtime is currently in use"),
        super::super::RemovalOutcome::RequiredBy(dependents) => bail!(
            "Qwen3 runtime cannot be repaired while required by installed components: {}",
            dependents.join(", ")
        ),
        super::super::RemovalOutcome::PreservedModified(paths) => bail!(
            "Qwen3 runtime cannot be repaired because {} modified managed file(s) were preserved",
            paths.len()
        ),
    }
}

fn install_delivery(
    delivery: &QwenRuntimeDelivery,
    cancel: &AtomicBool,
    on_progress: &impl Fn(u64, u64),
) -> Result<()> {
    let sequence = INSTALL_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let working_root = crate::paths::app_local_data_dir();
    let scratch = working_root.join("component-downloads");
    archive::ensure_working_directory(&working_root, &scratch)?;
    let archive_paths = delivery
        .archives
        .iter()
        .enumerate()
        .map(|(index, _)| {
            scratch.join(format!(
                "{COMPONENT_ID}-{index}-{}-{sequence}.download",
                std::process::id()
            ))
        })
        .collect::<Vec<_>>();
    let staging_parent = working_root.join("component-staging");
    archive::ensure_working_directory(&working_root, &staging_parent)?;
    let staging = staging_parent.join(format!(
        "{COMPONENT_ID}-{}-{}-{sequence}",
        delivery.version,
        std::process::id()
    ));
    std::fs::create_dir(&staging)?;
    let total_download = delivery.archives.iter().try_fold(0_u64, |total, archive| {
        total
            .checked_add(archive.size_bytes)
            .ok_or_else(|| anyhow!("Qwen3 download size overflow"))
    })?;

    let result = (|| {
        let mut progress_offset = 0_u64;
        for (index, (archive_delivery, archive_path)) in
            delivery.archives.iter().zip(&archive_paths).enumerate()
        {
            download_archive(
                archive_delivery,
                archive_path,
                cancel,
                progress_offset,
                total_download,
                on_progress,
            )?;
            archive::extract_archive(archive_path, index, &staging, delivery)?;
            progress_offset = progress_offset
                .checked_add(archive_delivery.size_bytes)
                .ok_or_else(|| anyhow!("Qwen3 download progress overflow"))?;
        }
        finish_staging(&staging, delivery)?;
        on_progress(total_download, total_download);
        Ok(())
    })();
    for path in &archive_paths {
        if path.is_file() {
            let _ = std::fs::remove_file(path);
        }
    }
    if staging.exists() {
        let _ = archive::cleanup_owned(&staging, delivery.files);
    }
    result
}

fn download_archive(
    archive: &QwenRuntimeArchive,
    target: &Path,
    cancel: &AtomicBool,
    progress_offset: u64,
    progress_total: u64,
    on_progress: &impl Fn(u64, u64),
) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        bail!("Download cancelled");
    }
    let response = crate::api::client::UREQ_DOWNLOAD_AGENT
        .get(archive.url)
        .header("User-Agent", "ScreenGoatedToolbox")
        .call()
        .map_err(|error| anyhow!("Qwen3 runtime download failed: {error}"))?;
    let headers = response.headers();
    let content_length = headers
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());
    if content_length != Some(archive.size_bytes) {
        bail!("Qwen3 runtime download size does not match this build");
    }
    let mut reader = response.into_body().into_reader();
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)?;
    let mut buffer = [0_u8; 256 * 1024];
    let mut downloaded = 0_u64;
    loop {
        if cancel.load(Ordering::Relaxed) {
            bail!("Download cancelled");
        }
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        downloaded = downloaded
            .checked_add(read as u64)
            .ok_or_else(|| anyhow!("Qwen3 runtime download is too large"))?;
        if downloaded > archive.size_bytes {
            bail!("Qwen3 runtime download exceeds its pinned size");
        }
        output.write_all(&buffer[..read])?;
        on_progress(progress_offset + downloaded, progress_total);
    }
    if downloaded != archive.size_bytes {
        bail!("Qwen3 runtime download is incomplete");
    }
    output.flush()?;
    output.sync_all()?;
    let expected = OwnedComponentFile {
        path: PathBuf::from("archive.download"),
        size_bytes: archive.size_bytes,
        sha256: archive.sha256.to_string(),
    };
    if !file_matches(target, &expected)? {
        bail!("Qwen3 runtime archive checksum mismatch");
    }
    Ok(())
}

fn adopt_legacy(
    delivery: &QwenRuntimeDelivery,
    cancel: &AtomicBool,
    on_progress: &impl Fn(u64, u64),
) -> Result<bool> {
    let legacy = crate::unpack_dlls::private_bin_dir();
    let reusable = delivery.files.iter().filter(|file| file.archive_index != 0);
    if !legacy.is_dir() || !legacy_files_match(&legacy, reusable)? {
        return Ok(false);
    }
    let sequence = INSTALL_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let working_root = crate::paths::app_local_data_dir();
    let scratch = working_root.join("component-downloads");
    archive::ensure_working_directory(&working_root, &scratch)?;
    let archive_path = scratch.join(format!(
        "{COMPONENT_ID}-adopt-runtime-{}-{sequence}.download",
        std::process::id()
    ));
    let staging_parent = working_root.join("component-staging");
    archive::ensure_working_directory(&working_root, &staging_parent)?;
    let staging = staging_parent.join(format!(
        "{COMPONENT_ID}-adopt-{}-{sequence}",
        std::process::id()
    ));
    std::fs::create_dir(&staging)?;
    let result = (|| {
        copy_legacy_files(
            &legacy,
            &staging,
            delivery.files.iter().filter(|file| file.archive_index != 0),
        )?;
        let runtime_archive = delivery
            .archives
            .first()
            .ok_or_else(|| anyhow!("Qwen3 delivery has no runtime asset"))?;
        download_archive(
            runtime_archive,
            &archive_path,
            cancel,
            0,
            runtime_archive.size_bytes,
            on_progress,
        )?;
        archive::extract_archive(&archive_path, 0, &staging, delivery)?;
        finish_staging(&staging, delivery)
    })();
    if archive_path.is_file() {
        let _ = std::fs::remove_file(&archive_path);
    }
    match result {
        Ok(()) => {
            cleanup_verified_legacy(&legacy, delivery);
            Ok(true)
        }
        Err(error) => {
            if staging.exists() {
                let _ = archive::cleanup_owned(&staging, delivery.files);
            }
            Err(error)
        }
    }
}

fn legacy_files_match<'a>(
    legacy: &Path,
    files: impl Iterator<Item = &'a super::QwenRuntimeFile>,
) -> Result<bool> {
    for file in files {
        let Some(name) = Path::new(file.path).file_name() else {
            return Ok(false);
        };
        let source = legacy.join(name);
        if !file_matches(&source, &owned_file(file)).unwrap_or(false) {
            return Ok(false);
        }
        if source
            .extension()
            .is_some_and(|extension| extension.to_string_lossy().eq_ignore_ascii_case("dll"))
        {
            archive::validate_x64_pe(&source)?;
        }
    }
    Ok(true)
}

fn copy_legacy_files<'a>(
    legacy: &Path,
    staging: &Path,
    files: impl Iterator<Item = &'a super::QwenRuntimeFile>,
) -> Result<()> {
    for file in files {
        let target = archive::prepare_target(staging, Path::new(file.path))?;
        let name = Path::new(file.path)
            .file_name()
            .expect("validated file name");
        let source = legacy.join(name);
        std::fs::hard_link(&source, &target).or_else(|_| {
            std::fs::copy(&source, &target)?;
            Ok::<(), std::io::Error>(())
        })?;
        if !file_matches(&target, &owned_file(file))? {
            bail!("adopted Qwen3 runtime file changed during staging");
        }
    }
    Ok(())
}

fn cleanup_verified_legacy(legacy: &Path, delivery: &QwenRuntimeDelivery) {
    for file in delivery.files {
        let Some(name) = Path::new(file.path).file_name() else {
            continue;
        };
        let path = legacy.join(name);
        if file_matches(&path, &owned_file(file)).unwrap_or(false) {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn finish_staging(staging: &Path, delivery: &QwenRuntimeDelivery) -> Result<()> {
    super::super::write_receipt(staging, &receipt(delivery))?;
    validate_exact_tree(staging, delivery.files)?;
    let target = version_root(delivery)?;
    let parent = super::super::ensure_component_parent(COMPONENT_ID)?;
    if target.parent() != Some(parent.as_path()) || target.exists() {
        bail!("Qwen3 runtime install target is invalid");
    }
    std::fs::rename(staging, &target)?;
    super::super::validate_version_root(COMPONENT_ID, delivery.version)?;
    validate_install(delivery)
}

fn receipt(delivery: &QwenRuntimeDelivery) -> ComponentReceipt {
    ComponentReceipt {
        schema_version: 1,
        id: COMPONENT_ID.to_string(),
        version: delivery.version.to_string(),
        architecture: ARCHITECTURE.to_string(),
        dependencies: vec![VC_COMPONENT_ID.to_string()],
        files: delivery.files.iter().map(owned_file).collect(),
    }
}

#[cfg(all(test, not(feature = "recorder-worker")))]
pub(super) fn adopt_from_for_test(
    legacy: &Path,
    staging: &Path,
    delivery: &QwenRuntimeDelivery,
) -> Result<bool> {
    if !legacy_files_match(legacy, delivery.files.iter())? {
        return Ok(false);
    }
    copy_legacy_files(legacy, staging, delivery.files.iter())?;
    Ok(true)
}
