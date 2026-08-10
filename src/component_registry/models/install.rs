use std::collections::HashSet;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

use anyhow::{Result, anyhow, bail};
use sha2::{Digest as _, Sha256};

use super::super::receipt::{ComponentReceipt, file_matches};
use super::{
    ARCHITECTURE, MAX_MODEL_FILES, ModelArchive, ModelDelivery, ModelFile, legacy_root, owned_file,
    staging, validate_exact_tree, validate_status, version_root,
};

static INSTALL_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));
static INSTALL_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(super) fn ensure(
    delivery: &ModelDelivery,
    cancelled: &AtomicBool,
    on_progress: impl Fn(u64, u64),
) -> Result<()> {
    let _guard = INSTALL_LOCK
        .lock()
        .unwrap_or_else(|value| value.into_inner());
    if validate_status(delivery).is_ok() {
        return Ok(());
    }
    let mutation = super::super::acquire_mutation_guard()?;
    if validate_status(delivery).is_ok() {
        return Ok(());
    }
    let target = version_root(delivery)?;
    if target.exists() {
        bail!(
            "{} is installed but failed integrity; remove it from Downloaded Tools before repair",
            delivery.id
        );
    }
    let _install_lease = super::super::acquire(&delivery.id)?;
    if validate_status(delivery).is_ok() {
        return Ok(());
    }
    install_delivery(delivery, cancelled, &on_progress, &mutation)
}

fn install_delivery(
    delivery: &ModelDelivery,
    cancelled: &AtomicBool,
    on_progress: &impl Fn(u64, u64),
    mutation: &super::super::RegistryMutationGuard,
) -> Result<()> {
    let (archive_path, staging_root) = working_paths(delivery, mutation)?;
    let entry_scratch = archive_path.with_extension("entry");
    let mut adopted_legacy = false;
    let result = (|| {
        adopted_legacy = adopt_legacy(delivery, &staging_root)?;
        if let Some(archive) = &delivery.archive {
            install_archive(
                delivery,
                archive,
                &archive_path,
                &staging_root,
                cancelled,
                on_progress,
            )?;
        } else {
            install_direct(
                delivery,
                &archive_path,
                &staging_root,
                cancelled,
                on_progress,
            )?;
        }
        if cancelled.load(Ordering::Relaxed) {
            bail!("model download cancelled");
        }
        publish(delivery, &staging_root)?;
        super::invalidate_status(&delivery.id);
        validate_status(delivery)
    })();
    for scratch in [&archive_path, &entry_scratch] {
        if scratch.is_file() {
            let _ = std::fs::remove_file(scratch);
        }
    }
    if result.is_err()
        && staging_root
            .read_dir()
            .is_ok_and(|mut entries| entries.next().is_none())
    {
        let _ = std::fs::remove_dir(&staging_root);
    }
    if result.is_ok() && adopted_legacy {
        cleanup_verified_legacy(delivery)?;
    }
    result
}

fn install_direct(
    delivery: &ModelDelivery,
    scratch_path: &Path,
    staging_root: &Path,
    cancelled: &AtomicBool,
    on_progress: &impl Fn(u64, u64),
) -> Result<()> {
    let total = delivery.installed_size_bytes;
    let mut completed = 0_u64;
    for file in &delivery.files {
        if cancelled.load(Ordering::Relaxed) {
            bail!("model download cancelled");
        }
        let target = staging::prepare_target(staging_root, &file.path)?;
        if file_matches(&target, &owned_file(file)).unwrap_or(false) {
            completed += file.size_bytes;
            on_progress(completed, total);
            continue;
        }
        let url = file
            .url
            .as_deref()
            .ok_or_else(|| anyhow!("direct model file has no immutable URL"))?;
        download(
            url,
            file.size_bytes,
            &file.sha256,
            scratch_path,
            cancelled,
            &|done| on_progress(completed + done, total),
        )?;
        std::fs::rename(scratch_path, &target)?;
        completed += file.size_bytes;
        on_progress(completed, total);
    }
    Ok(())
}

fn install_archive(
    delivery: &ModelDelivery,
    archive: &ModelArchive,
    archive_path: &Path,
    staging_root: &Path,
    cancelled: &AtomicBool,
    on_progress: &impl Fn(u64, u64),
) -> Result<()> {
    if delivery.files.iter().all(|file| {
        file_matches(&staging_root.join(&file.path), &owned_file(file)).unwrap_or(false)
    }) {
        on_progress(archive.size_bytes, archive.size_bytes);
        return Ok(());
    }
    download(
        &archive.url,
        archive.size_bytes,
        &archive.sha256,
        archive_path,
        cancelled,
        &|done| on_progress(done, archive.size_bytes),
    )?;
    extract_archive(
        archive_path,
        &archive_path.with_extension("entry"),
        staging_root,
        delivery,
        cancelled,
    )?;
    on_progress(archive.size_bytes, archive.size_bytes);
    Ok(())
}

fn download(
    url: &str,
    expected_size: u64,
    expected_sha256: &str,
    target: &Path,
    cancelled: &AtomicBool,
    on_progress: &impl Fn(u64),
) -> Result<()> {
    if cancelled.load(Ordering::Relaxed) {
        bail!("model download cancelled");
    }
    let response = crate::api::client::UREQ_DOWNLOAD_AGENT
        .get(url)
        .header("User-Agent", "ScreenGoatedToolbox")
        .call()
        .map_err(|error| anyhow!("model download failed: {error}"))?;
    if response
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .is_some_and(|size| size != expected_size)
    {
        bail!("model download size does not match this build");
    }
    let mut reader = response.into_body().into_reader();
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;
        options.share_mode(0);
    }
    let mut output = options.open(target)?;
    let mut buffer = [0_u8; 256 * 1024];
    let mut downloaded = 0_u64;
    let mut hasher = Sha256::new();
    loop {
        if cancelled.load(Ordering::Relaxed) {
            bail!("model download cancelled");
        }
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        downloaded = downloaded
            .checked_add(read as u64)
            .filter(|size| *size <= expected_size)
            .ok_or_else(|| anyhow!("model download exceeds its pinned size"))?;
        output.write_all(&buffer[..read])?;
        hasher.update(&buffer[..read]);
        on_progress(downloaded);
    }
    if downloaded != expected_size
        || !format!("{:x}", hasher.finalize()).eq_ignore_ascii_case(expected_sha256)
    {
        bail!("model download is incomplete");
    }
    output.flush()?;
    output.sync_all()?;
    Ok(())
}

fn extract_archive(
    archive_path: &Path,
    entry_scratch: &Path,
    staging_root: &Path,
    delivery: &ModelDelivery,
    cancelled: &AtomicBool,
) -> Result<()> {
    let mut archive = zip::ZipArchive::new(std::fs::File::open(archive_path)?)?;
    if archive.len() != delivery.files.len() || archive.len() > MAX_MODEL_FILES {
        bail!("model archive has an unexpected entry count");
    }
    let mut seen = HashSet::new();
    let mut expanded = 0_u64;
    for index in 0..archive.len() {
        if cancelled.load(Ordering::Relaxed) {
            bail!("model download cancelled");
        }
        let mut entry = archive.by_index(index)?;
        if entry.is_dir() || zip_entry_is_link(&entry) {
            bail!("model archive contains a link or unexpected directory");
        }
        let relative = entry
            .enclosed_name()
            .ok_or_else(|| anyhow!("model archive contains an unsafe path"))?
            .to_path_buf();
        let expected = delivery
            .files
            .iter()
            .find(|file| file.path == relative)
            .ok_or_else(|| anyhow!("model archive contains an unowned file"))?;
        if !seen.insert(relative.clone()) || entry.size() != expected.size_bytes {
            bail!("model archive entry does not match its inventory");
        }
        expanded = expanded
            .checked_add(entry.size())
            .filter(|size| *size <= delivery.installed_size_bytes)
            .ok_or_else(|| anyhow!("model archive expands beyond its pinned limit"))?;
        let target = staging::prepare_target(staging_root, &relative)?;
        if file_matches(&target, &owned_file(expected)).unwrap_or(false) {
            continue;
        }
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(entry_scratch)?;
        let mut hasher = Sha256::new();
        let mut copied = 0_u64;
        let mut buffer = [0_u8; 256 * 1024];
        loop {
            if cancelled.load(Ordering::Relaxed) {
                bail!("model download cancelled");
            }
            let read = entry.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            copied = copied
                .checked_add(read as u64)
                .filter(|size| *size <= expected.size_bytes)
                .ok_or_else(|| anyhow!("model archive entry exceeds its pinned size"))?;
            output.write_all(&buffer[..read])?;
            hasher.update(&buffer[..read]);
        }
        output.flush()?;
        output.sync_all()?;
        if copied != expected.size_bytes
            || !format!("{:x}", hasher.finalize()).eq_ignore_ascii_case(&expected.sha256)
        {
            bail!("model archive entry was truncated or oversized");
        }
        drop(output);
        std::fs::rename(entry_scratch, target)?;
    }
    if seen.len() != delivery.files.len() || expanded != delivery.installed_size_bytes {
        bail!("model archive inventory is incomplete");
    }
    Ok(())
}

fn zip_entry_is_link(entry: &zip::read::ZipFile<'_, std::fs::File>) -> bool {
    entry
        .unix_mode()
        .is_some_and(|mode| mode & 0o170000 != 0 && mode & 0o170000 != 0o100000)
}

fn adopt_legacy(delivery: &ModelDelivery, staging_root: &Path) -> Result<bool> {
    let Some(legacy) = legacy_root(delivery) else {
        return Ok(false);
    };
    let metadata = match std::fs::symlink_metadata(&legacy) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    if !metadata.is_dir() || super::super::receipt::is_reparse_point(&metadata) {
        return Ok(false);
    }
    let mut adopted = false;
    for file in &delivery.files {
        let source = super::super::receipt::resolve_owned_path(&legacy, &file.path)?;
        if !file_matches(&source, &owned_file(file)).unwrap_or(false) {
            continue;
        }
        let target = staging::prepare_target(staging_root, &file.path)?;
        if file_matches(&target, &owned_file(file)).unwrap_or(false) {
            adopted = true;
            continue;
        }
        std::fs::copy(&source, &target)?;
        verify_file(&target, file)?;
        adopted = true;
    }
    Ok(adopted)
}

fn cleanup_verified_legacy(delivery: &ModelDelivery) -> Result<()> {
    let Some(legacy) = legacy_root(delivery) else {
        return Ok(());
    };
    let metadata = match std::fs::symlink_metadata(&legacy) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if !metadata.is_dir() || super::super::receipt::is_reparse_point(&metadata) {
        bail!("legacy model root is unsafe");
    }
    for file in &delivery.files {
        let path = super::super::receipt::resolve_owned_path(&legacy, &file.path)?;
        if file_matches(&path, &owned_file(file)).unwrap_or(false) {
            std::fs::remove_file(&path)?;
            staging::remove_empty_parents(path.parent(), &legacy)?;
        }
    }
    let _ = std::fs::remove_dir(legacy);
    Ok(())
}

fn publish(delivery: &ModelDelivery, staging_root: &Path) -> Result<()> {
    super::super::write_receipt(staging_root, &receipt(delivery))?;
    validate_exact_tree(staging_root, &delivery.files)?;
    let target = version_root(delivery)?;
    let parent = super::super::ensure_component_parent(&delivery.id)?;
    if target.parent() != Some(parent.as_path()) || target.exists() {
        bail!("model component target is invalid or already exists");
    }
    std::fs::rename(staging_root, &target)?;
    super::super::validate_version_root(&delivery.id, &delivery.version)?;
    Ok(())
}

fn working_paths(
    delivery: &ModelDelivery,
    mutation: &super::super::RegistryMutationGuard,
) -> Result<(PathBuf, PathBuf)> {
    let sequence = INSTALL_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let root = super::state_root();
    let downloads = root.join("component-downloads");
    staging::ensure_directory_tree(&root, &downloads)?;
    super::auxiliary::cleanup_stale_downloads(&delivery.id, mutation)?;
    let archive = downloads.join(format!(
        "{}-{}-{sequence}.download",
        delivery.id,
        std::process::id()
    ));
    let staging_root = super::auxiliary::staging_root(delivery)?;
    let staging_parent = staging_root
        .parent()
        .ok_or_else(|| anyhow!("model staging root has no parent"))?;
    staging::ensure_directory_tree(&root, staging_parent)?;
    prepare_reusable_staging(delivery, &staging_root)?;
    Ok((archive, staging_root))
}

fn prepare_reusable_staging(delivery: &ModelDelivery, root: &Path) -> Result<()> {
    match std::fs::create_dir(root) {
        Ok(()) => return Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error.into()),
    }
    let metadata = std::fs::symlink_metadata(root)?;
    if !metadata.is_dir() || super::super::receipt::is_reparse_point(&metadata) {
        bail!("reusable model staging root is unsafe");
    }
    let mut actual = Vec::new();
    staging::collect_regular_files(root, root, &mut actual, MAX_MODEL_FILES + 1)?;
    for relative in actual {
        let path = super::super::receipt::resolve_owned_path(root, &relative)?;
        if relative == Path::new(super::super::receipt::RECEIPT_NAME) {
            let staged = ComponentReceipt::read(&path)?;
            if !receipt_matches(delivery, &staged) {
                bail!("reusable model staging contains a modified receipt; content was preserved");
            }
            std::fs::remove_file(path)?;
            continue;
        }
        let Some(expected) = delivery.files.iter().find(|file| file.path == relative) else {
            bail!("reusable model staging contains unknown content; content was preserved");
        };
        if !file_matches(&path, &owned_file(expected)).unwrap_or(false) {
            bail!("reusable model staging contains modified content; content was preserved");
        }
    }
    Ok(())
}

fn receipt_matches(delivery: &ModelDelivery, staged: &ComponentReceipt) -> bool {
    staged.schema_version == 1
        && staged.id == delivery.id
        && staged.version == delivery.version
        && staged.architecture == ARCHITECTURE
        && staged.dependencies.is_empty()
        && staged.files.len() == delivery.files.len()
        && staged
            .files
            .iter()
            .zip(&delivery.files)
            .all(|(owned, expected)| {
                owned.path == expected.path
                    && owned.size_bytes == expected.size_bytes
                    && owned.sha256.eq_ignore_ascii_case(&expected.sha256)
            })
}

fn verify_file(path: &Path, file: &ModelFile) -> Result<()> {
    if !file_matches(path, &super::owned_file(file))? {
        bail!(
            "model file failed integrity verification: {}",
            file.path.display()
        );
    }
    Ok(())
}

fn receipt(delivery: &ModelDelivery) -> ComponentReceipt {
    ComponentReceipt {
        schema_version: 1,
        id: delivery.id.clone(),
        version: delivery.version.clone(),
        architecture: ARCHITECTURE.to_string(),
        dependencies: Vec::new(),
        files: delivery.files.iter().map(super::owned_file).collect(),
    }
}

#[cfg(all(test, not(feature = "recorder-worker")))]
pub(super) fn adopt_from_for_test(
    delivery: &ModelDelivery,
    legacy: &Path,
    staging_root: &Path,
) -> Result<usize> {
    let mut adopted = 0;
    for file in &delivery.files {
        let source = super::super::receipt::resolve_owned_path(legacy, &file.path)?;
        if !file_matches(&source, &owned_file(file)).unwrap_or(false) {
            continue;
        }
        let target = staging::prepare_target(staging_root, &file.path)?;
        std::fs::copy(source, &target)?;
        verify_file(&target, file)?;
        adopted += 1;
    }
    Ok(adopted)
}

#[cfg(all(test, not(feature = "recorder-worker")))]
pub(super) fn extract_for_test(
    archive: &Path,
    staging_root: &Path,
    delivery: &ModelDelivery,
    cancelled: &AtomicBool,
) -> Result<()> {
    let scratch = archive.with_extension("entry");
    let result = extract_archive(archive, &scratch, staging_root, delivery, cancelled);
    if scratch.is_file() {
        let _ = std::fs::remove_file(scratch);
    }
    result
}
