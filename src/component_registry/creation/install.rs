use crate::component_registry::verified_archive::{self, FileSpec, Policy};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use anyhow::{Result, anyhow, bail};

use super::*;
use crate::component_registry::persisted_download;
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
    let report = |downloaded: u64, total: u64| {
        set_progress(downloaded as f32 / total as f32 * 100.0, &name);
        if let Some(badge) = &badge {
            badge.report(downloaded, total)
        }
    };
    // The transfer holds only this component's transfer guard; the registry
    // guard covers extraction and publication.
    let result = prefetch(delivery, &stop, &report).and_then(|()| {
        let _mutation = super::super::acquire_mutation_guard()?;
        let mut install_lease = super::super::acquire(COMPONENT_ID)?;
        install_delivery(delivery, &stop, report, &mut install_lease)
    });
    finish_progress(result.is_ok());
    if let Some(badge) = &badge {
        badge.finish()
    }
    if result.is_ok() && use_badge {
        notify_success(&name)
    }
    result
}

fn install_delivery(
    delivery: &CreationDelivery,
    stop: &AtomicBool,
    on_progress: impl Fn(u64, u64),
    install_lease: &mut ComponentLease,
) -> Result<()> {
    let sequence = INSTALL_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let staging_parent = crate::paths::app_runtime_local_data_dir().join("component-staging");
    ensure_directory(&staging_parent)?;
    let staging = staging_parent.join(format!(
        "{COMPONENT_ID}-{}-{}-{sequence}",
        delivery.version,
        std::process::id()
    ));
    std::fs::create_dir(&staging)?;
    let result = (|| {
        let archive =
            persisted_download::fetch(COMPONENT_ID, &pinned_body(delivery), stop, &on_progress)
                .map_err(|error| anyhow!("Creation download failed: {error:#}"))?;
        validate_archive(&archive, delivery)?;
        extract_archive(&archive, &staging, delivery, stop)?;
        super::super::write_receipt(&staging, &receipt(delivery))?;
        validation::validate_exact_tree(&staging, delivery)?;
        if stop.load(Ordering::Acquire) {
            bail!("Creation installation was cancelled");
        }
        super::super::publication::publish_verified(
            COMPONENT_ID,
            delivery.version,
            &receipt(delivery).files,
            &staging,
            install_lease,
        )?;
        validation::validate_install(delivery)?;
        // A failed install keeps the received archive for the next attempt.
        persisted_download::discard_leftovers(COMPONENT_ID);
        Ok(())
    })();
    if staging.exists() {
        let mut paths = delivery
            .files
            .iter()
            .map(|file| file.path)
            .collect::<Vec<_>>();
        paths.push(super::super::receipt::RECEIPT_NAME);
        let _ = staging::cleanup_owned(&staging, &paths);
    }
    result
}

/// Transfers the archive of a fresh install while holding only this
/// component's transfer guard, before the caller takes the registry guard.
fn prefetch(
    delivery: &CreationDelivery,
    stop: &AtomicBool,
    on_progress: &impl Fn(u64, u64),
) -> Result<()> {
    persisted_download::prefetch(COMPONENT_ID, &[pinned_body(delivery)], stop, on_progress)
        .map_err(|error| anyhow!("Creation download failed: {error:#}"))
}

fn pinned_body(delivery: &CreationDelivery) -> persisted_download::PinnedBody<'_> {
    persisted_download::PinnedBody {
        url: delivery.download_url,
        size: delivery.size_bytes,
        sha256: delivery.sha256,
    }
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

fn extract_archive(
    path: &Path,
    staging: &Path,
    delivery: &CreationDelivery,
    stop: &AtomicBool,
) -> Result<()> {
    let files = delivery
        .files
        .iter()
        .map(|file| FileSpec::same(Path::new(file.path), file.size_bytes, file.sha256))
        .collect::<Vec<_>>();
    verified_archive::extract(
        std::fs::File::open(path)?,
        &files,
        Policy::files(MAX_ARCHIVE_ENTRIES, delivery.unpacked_size_bytes),
        stop,
        |index, member| {
            let expected = &delivery.files[index];
            let (target, _directories) =
                staging::prepare_target(staging, Path::new(expected.path))?;
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
