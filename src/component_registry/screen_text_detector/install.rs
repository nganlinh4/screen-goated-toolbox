pub(super) use super::super::native_file::validate_x64_pe;
use crate::component_registry::verified_archive::{self, FileSpec, Policy};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

use anyhow::{Context, Result, bail};

use super::{
    DetectorDelivery, ID, MAX_COMPONENT_FILES, receipt, validate_exact_tree, validate_install,
};
use crate::component_registry::receipt::{file_matches, is_reparse_point};
use crate::component_registry::{persisted_download, staging};

static INSTALL_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));
static INSTALL_SEQUENCE: AtomicU64 = AtomicU64::new(0);

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
    let mut install_lease = crate::component_registry::acquire(ID)?;
    install_delivery(delivery, cancelled, on_progress, &mut install_lease)
}

fn install_delivery(
    delivery: &DetectorDelivery,
    cancelled: &AtomicBool,
    on_progress: impl Fn(u64, u64),
    install_lease: &mut crate::component_registry::ComponentLease,
) -> Result<()> {
    let sequence = INSTALL_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let app_data = crate::paths::app_runtime_local_data_dir();
    let staging_parent = app_data.join("component-staging");
    staging::ensure_directory_tree(&app_data, &staging_parent)?;
    let stage = staging_parent.join(format!(
        "{ID}-{}-{}-{sequence}",
        delivery.version,
        std::process::id()
    ));
    std::fs::create_dir(&stage)?;

    let result = (|| {
        on_progress(0, delivery.size_bytes);
        let archive =
            persisted_download::fetch(ID, &pinned_body(delivery), cancelled, &on_progress)
                .context("Screen Translate detector download failed")?;
        validate_archive(delivery, &archive)?;
        extract(delivery, &archive, &stage, cancelled)?;
        validate_staging(delivery, &stage)?;
        crate::component_registry::write_receipt(&stage, &receipt(delivery))?;
        if cancelled.load(Ordering::Acquire) {
            bail!("detector installation was cancelled");
        }
        crate::component_registry::publication::publish_verified(
            ID,
            delivery.version,
            &receipt(delivery).files,
            &stage,
            install_lease,
        )?;
        validate_install(delivery)?;
        // A failed install keeps the received archive for the next attempt.
        persisted_download::discard_leftovers(ID);
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

/// Transfers the detector archive of a fresh install while holding only this
/// component's transfer guard, so the caller runs it before the registry
/// mutation guard.
pub(super) fn prefetch(
    delivery: &DetectorDelivery,
    cancelled: &AtomicBool,
    on_progress: &impl Fn(u64, u64),
) -> Result<()> {
    if validate_install(delivery).is_ok() {
        return Ok(());
    }
    on_progress(0, delivery.size_bytes);
    persisted_download::prefetch(ID, &[pinned_body(delivery)], cancelled, on_progress)
        .context("Screen Translate detector download failed")
}

fn pinned_body(delivery: &DetectorDelivery) -> persisted_download::PinnedBody<'_> {
    persisted_download::PinnedBody {
        url: delivery.download_url,
        size: delivery.size_bytes,
        sha256: delivery.sha256,
    }
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

fn extract(
    delivery: &DetectorDelivery,
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
            if !file_matches(&target, &super::owned_file(expected))? {
                bail!("extracted component file failed integrity verification");
            }

            Ok(())
        },
    )
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
