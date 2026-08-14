use std::collections::HashSet;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use anyhow::{Context, Result, anyhow, bail};

use super::{
    MAX_COMPONENT_FILES, RecorderDelivery, WORKER_ID, owned_file, receipt, recovery, staging,
    validate_install, version_root,
};
use crate::component_registry::RemovalOutcome;
use crate::component_registry::receipt::{file_matches, is_reparse_point};

static INSTALL_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));
static INSTALL_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(super) fn ensure_pair(
    web: &'static RecorderDelivery,
    worker: &'static RecorderDelivery,
    cancelled: &AtomicBool,
    on_progress: impl Fn(u64, u64),
) -> Result<()> {
    let _guard = INSTALL_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if validate_install(web).is_ok() && validate_install(worker).is_ok() {
        return Ok(());
    }

    let worker_failure = validate_install(worker)
        .err()
        .map(|error| format!("{error:#}"));
    let web_failure = validate_install(web)
        .err()
        .map(|error| format!("{error:#}"));
    if let Some(reason) = worker_failure.as_deref() {
        recovery::quarantine_invalid(worker, reason)?;
    }
    if let Some(reason) = web_failure.as_deref() {
        // A valid worker receipt depends on recorder-web and intentionally
        // blocks its removal, so repair the dependent first.
        if worker_failure.is_none() {
            clear_component(WORKER_ID)?;
        }
        recovery::quarantine_invalid(web, reason)?;
    }

    let total = web.size_bytes.saturating_add(worker.size_bytes);
    let mut completed = 0_u64;
    for delivery in [web, worker] {
        if validate_install(delivery).is_ok() {
            completed = completed.saturating_add(delivery.size_bytes);
            on_progress(completed, total);
            continue;
        }
        let _install_lease = crate::component_registry::acquire(delivery.id)?;
        install_delivery(delivery, cancelled, |done, _| {
            on_progress(completed.saturating_add(done), total)
        })?;
        completed = completed.saturating_add(delivery.size_bytes);
        on_progress(completed, total);
    }
    Ok(())
}

fn clear_component(id: &str) -> Result<()> {
    match crate::component_registry::request_remove(id)? {
        RemovalOutcome::Missing | RemovalOutcome::Removed => Ok(()),
        RemovalOutcome::Pending => bail!("{id} is currently in use"),
        RemovalOutcome::PreservedModified(paths) => bail!(
            "{id} cannot be repaired because {} modified managed file(s) were preserved",
            paths.len()
        ),
        RemovalOutcome::RequiredBy(dependents) => {
            bail!("{id} repair is blocked by {}", dependents.join(", "))
        }
    }
}

fn install_delivery(
    delivery: &RecorderDelivery,
    cancelled: &AtomicBool,
    on_progress: impl Fn(u64, u64),
) -> Result<()> {
    let sequence = INSTALL_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let scratch = crate::paths::app_local_data_dir().join("component-downloads");
    staging::ensure_directory_tree(&scratch, &scratch)?;
    let archive = scratch.join(format!(
        "{}-{}-{sequence}.download",
        delivery.id,
        std::process::id()
    ));
    let staging_parent = crate::paths::app_local_data_dir().join("component-staging");
    staging::ensure_directory_tree(&staging_parent, &staging_parent)?;
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
        extract(delivery, &archive, &stage)?;
        validate_staging(delivery, &stage)?;
        crate::component_registry::write_receipt(&stage, &receipt(delivery))?;
        let parent = crate::component_registry::ensure_component_parent(delivery.id)?;
        let target = version_root(delivery)?;
        if target.parent() != Some(parent.as_path()) || target.exists() {
            bail!("recorder component install target is invalid");
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
    delivery: &RecorderDelivery,
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
        bail!("recorder component download size does not match this build");
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
            bail!("recorder component download was cancelled");
        }
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        downloaded = downloaded
            .checked_add(read as u64)
            .ok_or_else(|| anyhow!("recorder component download is too large"))?;
        if downloaded > delivery.size_bytes {
            bail!("recorder component download exceeds its exact size");
        }
        output.write_all(&buffer[..read])?;
        on_progress(downloaded, delivery.size_bytes);
    }
    if downloaded != delivery.size_bytes {
        bail!("recorder component download is incomplete");
    }
    output.flush()?;
    output.sync_all()?;
    Ok(())
}

fn validate_archive(delivery: &RecorderDelivery, path: &Path) -> Result<()> {
    let expected = crate::component_registry::OwnedComponentFile {
        path: delivery.asset.into(),
        size_bytes: delivery.size_bytes,
        sha256: delivery.sha256.to_string(),
    };
    if !file_matches(path, &expected)? {
        bail!("recorder component archive checksum mismatch");
    }
    Ok(())
}

fn extract(delivery: &RecorderDelivery, archive_path: &Path, stage: &Path) -> Result<()> {
    let file = std::fs::File::open(archive_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    if archive.len() != delivery.files.len() || archive.len() > MAX_COMPONENT_FILES {
        bail!("recorder component archive has an unexpected entry count");
    }
    let mut extracted = HashSet::new();
    let mut extracted_bytes = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        if entry.is_dir() {
            bail!("recorder component archive contains a directory entry");
        }
        let relative = entry
            .enclosed_name()
            .ok_or_else(|| anyhow!("recorder component archive path is unsafe"))?
            .to_path_buf();
        let expected = delivery
            .files
            .iter()
            .find(|file| Path::new(file.path) == relative)
            .ok_or_else(|| anyhow!("recorder archive contains an unowned file"))?;
        if !extracted.insert(relative.clone()) || entry.size() != expected.size_bytes {
            bail!("recorder component archive entry does not match its manifest");
        }
        extracted_bytes = extracted_bytes
            .checked_add(entry.size())
            .ok_or_else(|| anyhow!("recorder component expands beyond its limit"))?;
        if extracted_bytes > delivery.unpacked_size_bytes {
            bail!("recorder component expands beyond its declared size");
        }
        let target = staging::prepare_target(stage, &relative)?;
        let mut output = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&target)?;
        std::io::copy(&mut entry, &mut output)?;
        output.flush()?;
        if !file_matches(&target, &owned_file(expected))? {
            bail!("recorder component extracted checksum mismatch");
        }
    }
    if extracted.len() != delivery.files.len() || extracted_bytes != delivery.unpacked_size_bytes {
        bail!("recorder component archive is incomplete");
    }
    Ok(())
}

fn validate_staging(delivery: &RecorderDelivery, stage: &Path) -> Result<()> {
    for file in delivery.files {
        if !file_matches(&stage.join(file.path), &owned_file(file))? {
            bail!("staged recorder component failed integrity verification");
        }
    }
    let mut actual = Vec::new();
    staging::collect_regular_files(stage, stage, &mut actual, MAX_COMPONENT_FILES + 1)?;
    if actual.len() != delivery.files.len()
        || actual.iter().any(|path| {
            !delivery
                .files
                .iter()
                .any(|file| Path::new(file.path) == path)
        })
    {
        bail!("staged recorder component contains unowned files");
    }
    if std::fs::symlink_metadata(stage)
        .map(|metadata| is_reparse_point(&metadata))
        .unwrap_or(true)
    {
        bail!("staged recorder component root is unsafe");
    }
    Ok(())
}
