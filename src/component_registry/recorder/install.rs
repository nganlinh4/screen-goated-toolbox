use crate::component_registry::verified_archive::{self, FileSpec, Policy};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use anyhow::{Context, Result, bail};

use super::{
    MAX_COMPONENT_FILES, RecorderDelivery, owned_file, receipt, recovery, staging,
    validate_install, validate_status, version_root,
};
use crate::component_registry::persisted_download;
use crate::component_registry::receipt::{file_matches, is_reparse_point};

static INSTALL_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));
static INSTALL_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(super) fn ensure_group(
    deliveries: &[&'static RecorderDelivery],
    cancelled: &AtomicBool,
    on_progress: impl Fn(u64, u64),
) -> Result<()> {
    if deliveries
        .iter()
        .all(|delivery| validate_status(delivery).is_ok())
    {
        return Ok(());
    }

    repair_group(deliveries, cancelled, on_progress)
}

pub(super) fn repair_group(
    deliveries: &[&'static RecorderDelivery],
    cancelled: &AtomicBool,
    on_progress: impl Fn(u64, u64),
) -> Result<()> {
    let _guard = INSTALL_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    let total = deliveries.iter().fold(0_u64, |sum, delivery| {
        sum.saturating_add(delivery.size_bytes)
    });
    let mut completed = 0_u64;
    for delivery in deliveries.iter().copied() {
        if validate_install(delivery).is_ok() {
            completed = completed.saturating_add(delivery.size_bytes);
            on_progress(completed, total);
            continue;
        }
        let mut install_lease = crate::component_registry::acquire(delivery.id)?;
        install_delivery(
            delivery,
            cancelled,
            |done, _| on_progress(completed.saturating_add(done), total),
            &mut install_lease,
        )?;
        completed = completed.saturating_add(delivery.size_bytes);
        on_progress(completed, total);
    }
    Ok(())
}

fn install_delivery(
    delivery: &RecorderDelivery,
    cancelled: &AtomicBool,
    on_progress: impl Fn(u64, u64),
    install_lease: &mut crate::component_registry::ComponentLease,
) -> Result<()> {
    let sequence = INSTALL_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let staging_parent = crate::paths::app_runtime_local_data_dir().join("component-staging");
    staging::ensure_directory_tree(&staging_parent, &staging_parent)?;
    let stage = staging_parent.join(format!(
        "{}-{}-{}-{sequence}",
        delivery.id,
        delivery.version,
        std::process::id()
    ));
    std::fs::create_dir(&stage)?;

    let result = (|| {
        let archive =
            persisted_download::fetch(delivery.id, &pinned_body(delivery), cancelled, &on_progress)
                .with_context(|| format!("{} download failed", delivery.id))?;
        validate_archive(delivery, &archive)?;
        extract(delivery, &archive, &stage, cancelled)?;
        validate_staging(delivery, &stage)?;
        crate::component_registry::write_receipt(&stage, &receipt(delivery))?;
        if cancelled.load(Ordering::Acquire) {
            bail!("recorder installation was cancelled");
        }
        let parent = crate::component_registry::ensure_component_parent(delivery.id)?;
        let target = version_root(delivery)?;
        if target.parent() != Some(parent.as_path()) {
            bail!("recorder component install target is invalid");
        }
        match std::fs::symlink_metadata(&target) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                crate::component_registry::recovery::no_replace_move(&stage, &target)?;
            }
            Err(error) => return Err(error.into()),
            Ok(_) => {
                let exclusive =
                    crate::component_registry::lease::upgrade_to_exclusive(install_lease)?;
                recovery::quarantine_invalid_reserved(
                    delivery,
                    "component integrity validation failed",
                    &exclusive,
                )?;
                crate::component_registry::recovery::no_replace_move(&stage, &target)?;
            }
        }
        validate_install(delivery)?;
        // A failed install keeps the received archive for the next attempt.
        persisted_download::discard_leftovers(delivery.id);
        Ok(())
    })();
    if stage.exists() {
        let mut owned = delivery
            .files
            .iter()
            .map(|file| PathBuf::from(file.path))
            .collect::<Vec<_>>();
        owned.push(PathBuf::from(
            crate::component_registry::receipt::RECEIPT_NAME,
        ));
        let _ = staging::cleanup_owned(&stage, &owned);
    }
    result
}

/// Whether a fresh install of any group member needs a network transfer.
pub(super) fn needs_transfer(deliveries: &[&'static RecorderDelivery]) -> bool {
    deliveries
        .iter()
        .any(|delivery| validate_status(delivery).is_err())
}

/// Transfers the archives of group members that are not installed while
/// holding only each component's transfer guard, so the caller runs it before
/// the registry mutation guard. Repairs of installed members download under
/// the guard, as before.
pub(super) fn prefetch_group(
    deliveries: &[&'static RecorderDelivery],
    cancelled: &AtomicBool,
    on_progress: &dyn Fn(u64, u64),
) -> Result<()> {
    let missing = deliveries
        .iter()
        .copied()
        .filter(|delivery| validate_status(delivery).is_err())
        .collect::<Vec<_>>();
    let total = missing.iter().fold(0_u64, |sum, delivery| {
        sum.saturating_add(delivery.size_bytes)
    });
    let mut completed = 0_u64;
    for delivery in missing {
        persisted_download::prefetch(
            delivery.id,
            &[pinned_body(delivery)],
            cancelled,
            &|done, _| on_progress(completed.saturating_add(done), total),
        )
        .with_context(|| format!("{} download failed", delivery.id))?;
        completed = completed.saturating_add(delivery.size_bytes);
    }
    Ok(())
}

fn pinned_body(delivery: &RecorderDelivery) -> persisted_download::PinnedBody<'_> {
    persisted_download::PinnedBody {
        url: delivery.download_url,
        size: delivery.size_bytes,
        sha256: delivery.sha256,
    }
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

fn extract(
    delivery: &RecorderDelivery,
    archive_path: &Path,
    stage: &Path,
    cancelled: &AtomicBool,
) -> Result<()> {
    let files = delivery
        .files
        .iter()
        .map(|file| FileSpec::same(Path::new(file.path), file.size_bytes, file.sha256))
        .collect::<Vec<_>>();
    verified_archive::extract(
        std::fs::File::open(archive_path)?,
        &files,
        Policy::files(MAX_COMPONENT_FILES, delivery.unpacked_size_bytes),
        cancelled,
        |index, member| {
            let expected = &delivery.files[index];
            let (target, _directories) = staging::prepare_target(stage, Path::new(expected.path))?;
            let mut output = std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&target)?;
            member.copy_to(&mut output, &|_| {})?;
            output.flush()?;
            drop(output);
            if !file_matches(&target, &owned_file(expected))? {
                bail!("extracted component file failed integrity verification");
            }

            Ok(())
        },
    )
}

fn validate_staging(delivery: &RecorderDelivery, stage: &Path) -> Result<()> {
    for file in delivery.files {
        if !file_matches(&stage.join(file.path), &owned_file(file))? {
            bail!("staged recorder component failed integrity verification");
        }
    }
    let actual = crate::component_registry::owned_tree::regular_files(
        stage,
        "recorder component",
        MAX_COMPONENT_FILES + 1,
    )?;
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
