//! Verified delivery foundation for the shared Windows x64 VC support runtime.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
#[cfg(not(feature = "recorder-worker"))]
use std::sync::atomic::{AtomicU32, Ordering};
#[cfg(not(feature = "recorder-worker"))]
use std::sync::{LazyLock, Mutex};

use anyhow::{Result, anyhow, bail};

use super::ComponentLease;
#[cfg(not(feature = "recorder-worker"))]
use super::RemovalOutcome;
#[cfg(not(feature = "recorder-worker"))]
use super::receipt::file_size_matches;
use super::receipt::{
    ComponentReceipt, OwnedComponentFile, RECEIPT_NAME, file_matches, resolve_owned_path,
};

mod install;
mod staging;
#[cfg(not(feature = "recorder-worker"))]
mod update;

const COMPONENT_ID: &str = "vc14-x64-runtime";
const ARCHITECTURE: &str = "x64";
const DISPLAY_NAME: &str = "Microsoft VC runtime support";
const MAX_COMPONENT_FILES: usize = 16;

#[cfg(not(feature = "recorder-worker"))]
#[derive(Clone, Debug)]
pub(crate) enum VcRuntimeStatus {
    Installed { bytes: u64, version: String },
    Installing { progress: f32 },
    Missing,
    Unavailable,
    Error(String),
}

#[derive(Clone, Copy)]
struct VcRuntimeFile {
    path: &'static str,
    size_bytes: u64,
    sha256: &'static str,
}

struct VcRuntimeDelivery {
    version: &'static str,
    asset: &'static str,
    download_url: &'static str,
    size_bytes: u64,
    sha256: &'static str,
    unpacked_size_bytes: u64,
    files: &'static [VcRuntimeFile],
}

include!(concat!(env!("OUT_DIR"), "/vc_runtime_delivery.rs"));

#[cfg(not(feature = "recorder-worker"))]
static INSTALLING: AtomicBool = AtomicBool::new(false);
#[cfg(not(feature = "recorder-worker"))]
static PROGRESS_BASIS_POINTS: AtomicU32 = AtomicU32::new(0);
#[cfg(not(feature = "recorder-worker"))]
static LAST_NOTICE: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));

pub(crate) struct VcRuntimeUse {
    bin_dir: PathBuf,
    _files: Vec<std::fs::File>,
    _lease: ComponentLease,
}

impl VcRuntimeUse {
    #[cfg(not(feature = "recorder-worker"))]
    pub(crate) fn bin_dir(&self) -> &Path {
        &self.bin_dir
    }
}

pub(crate) fn ensure_component(on_progress: impl Fn(u64, u64)) -> Result<VcRuntimeUse> {
    ensure_component_with_cancel(&std::sync::Arc::new(AtomicBool::new(false)), on_progress)
}

pub(crate) fn ensure_component_with_cancel(
    cancelled: &std::sync::Arc<AtomicBool>,
    on_progress: impl Fn(u64, u64),
) -> Result<VcRuntimeUse> {
    let delivery = delivery()?;
    let mut activity = None;
    if validate_install(delivery).is_err() {
        activity = Some(crate::install_activity::register(cancelled.clone())?);
        // The transfer holds only this component's transfer guard.
        install::prefetch(delivery, cancelled, &on_progress)?;
    }
    let _mutation = super::acquire_mutation_guard_cancellable(cancelled)?;
    if validate_install(delivery).is_err() {
        let _activity = match activity.take() {
            Some(activity) => activity,
            None => crate::install_activity::register(cancelled.clone())?,
        };
        install::install(cancelled, on_progress)?;
    }
    let lease = super::acquire(COMPONENT_ID)?;
    let root = version_root(delivery)?;
    let files = lock_component_files(&root, delivery.files)?;
    validate_install(delivery)?;
    Ok(VcRuntimeUse {
        bin_dir: root.join("bin/x64"),
        _files: files,
        _lease: lease,
    })
}

#[cfg(not(feature = "recorder-worker"))]
pub(crate) fn current_status() -> VcRuntimeStatus {
    if INSTALLING.load(Ordering::Acquire) {
        return VcRuntimeStatus::Installing {
            progress: PROGRESS_BASIS_POINTS.load(Ordering::Relaxed) as f32 / 100.0,
        };
    }

    let Some(delivery) = optional_delivery() else {
        return VcRuntimeStatus::Unavailable;
    };
    match validate_status(delivery) {
        Ok(()) => VcRuntimeStatus::Installed {
            bytes: delivery.unpacked_size_bytes,
            version: delivery.version.to_string(),
        },
        Err(error) if version_root(delivery).is_ok_and(|root| root.exists()) => {
            VcRuntimeStatus::Error(error.to_string())
        }
        Err(_) => VcRuntimeStatus::Missing,
    }
}

#[cfg(not(feature = "recorder-worker"))]
pub(crate) fn current_notice() -> Option<String> {
    LAST_NOTICE
        .lock()
        .unwrap_or_else(|value| value.into_inner())
        .clone()
}

#[cfg(not(feature = "recorder-worker"))]
pub(crate) fn version_label() -> Option<String> {
    optional_delivery().map(|delivery| format!("{} ({ARCHITECTURE})", delivery.version))
}

#[cfg(not(feature = "recorder-worker"))]
pub(crate) fn start_install() -> bool {
    let status = current_status();
    if matches!(
        status,
        VcRuntimeStatus::Installed { .. } | VcRuntimeStatus::Installing { .. }
    ) || INSTALLING
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return false;
    }
    PROGRESS_BASIS_POINTS.store(0, Ordering::Relaxed);
    crate::task_runtime::spawn_detached(
        crate::task_runtime::TaskClass::Io,
        "vc-runtime-install",
        || {
            let language = crate::APP
                .lock()
                .map(|app| app.config.ui_language.clone())
                .unwrap_or_else(|_| "en".to_string());
            let component_name = crate::gui::locale::LocaleText::get(&language)
                .auxiliary
                .managed_tools
                .tool_vc_runtime
                .to_string();
            let badge =
                crate::overlay::auto_copy_badge::DownloadProgressBadge::new(&component_name);
            let result = ensure_component(|downloaded, total| {
                badge.report(downloaded, total);
                let basis_points = downloaded
                    .saturating_mul(10_000)
                    .checked_div(total.max(1))
                    .unwrap_or(0)
                    .min(10_000) as u32;
                PROGRESS_BASIS_POINTS.store(basis_points, Ordering::Relaxed);
            })
            .map(|support| {
                crate::log_info!(
                    "[Components] VC runtime support ready at {}",
                    support.bin_dir().display()
                );
            });
            let mut notice = LAST_NOTICE
                .lock()
                .unwrap_or_else(|value| value.into_inner());
            *notice = result.as_ref().err().map(ToString::to_string);
            drop(notice);
            INSTALLING.store(false, Ordering::Release);
            let locale = crate::overlay::auto_copy_badge::locale_text();
            match result {
                Ok(()) => {
                    let title = crate::overlay::auto_copy_badge::format_locale(
                        locale.component_installed_fmt,
                        &[("name", &component_name)],
                    );
                    crate::overlay::auto_copy_badge::show_detailed_notification(
                        &title,
                        &component_name,
                        crate::overlay::auto_copy_badge::NotificationType::Success,
                    );
                }
                Err(error) => {
                    let title = crate::overlay::auto_copy_badge::format_locale(
                        locale.component_install_failed_fmt,
                        &[("name", &component_name)],
                    );
                    crate::overlay::auto_copy_badge::show_detailed_notification(
                        &title,
                        &error.to_string(),
                        crate::overlay::auto_copy_badge::NotificationType::Error,
                    );
                }
            }
        },
    );
    true
}

#[cfg(not(feature = "recorder-worker"))]
pub(crate) fn remove() -> Result<()> {
    let _owners = crate::overlay::component_removal::stop_audio_owners()?;
    let result = match super::request_remove_and_wait(COMPONENT_ID)? {
        RemovalOutcome::Missing | RemovalOutcome::Removed | RemovalOutcome::Pending => Ok(()),
        RemovalOutcome::RequiredBy(dependents) => bail!(
            "{DISPLAY_NAME} is required by installed components: {}",
            dependents.join(", ")
        ),
        RemovalOutcome::PreservedModified(paths) => bail!(
            "{DISPLAY_NAME} contains {} unrecorded or unsafe path(s); they were preserved",
            paths.len()
        ),
    };
    *LAST_NOTICE
        .lock()
        .unwrap_or_else(|value| value.into_inner()) =
        result.as_ref().err().map(ToString::to_string);
    result
}

fn delivery() -> Result<&'static VcRuntimeDelivery> {
    optional_delivery()
        .ok_or_else(|| anyhow!("verified {DISPLAY_NAME} download contract is unavailable"))
}

fn optional_delivery() -> Option<&'static VcRuntimeDelivery> {
    #[cfg(not(feature = "recorder-worker"))]
    if let Some(delivery) = update::delivery() {
        return Some(delivery);
    }
    VC_RUNTIME_DELIVERY.as_ref()
}

fn version_root(delivery: &VcRuntimeDelivery) -> Result<PathBuf> {
    super::component_version_root(COMPONENT_ID, delivery.version)
}

fn validate_install(delivery: &VcRuntimeDelivery) -> Result<()> {
    validate_receipt(delivery, file_matches)
}

#[cfg(not(feature = "recorder-worker"))]
fn validate_status(delivery: &VcRuntimeDelivery) -> Result<()> {
    validate_receipt(delivery, file_size_matches)
}

fn validate_receipt(
    delivery: &VcRuntimeDelivery,
    matches: fn(&Path, &OwnedComponentFile) -> Result<bool>,
) -> Result<()> {
    let root = super::validate_version_root(COMPONENT_ID, delivery.version)?;
    let receipt = ComponentReceipt::read(&root.join(RECEIPT_NAME))?;
    if receipt.id != COMPONENT_ID
        || receipt.version != delivery.version
        || receipt.architecture != ARCHITECTURE
        || !receipt.dependencies.is_empty()
        || receipt.files.len() != delivery.files.len()
    {
        bail!("VC runtime receipt does not match this build");
    }
    for (receipt_file, expected) in receipt.files.iter().zip(delivery.files) {
        let owned = owned_file(expected);
        if receipt_file.path != owned.path
            || receipt_file.size_bytes != owned.size_bytes
            || !receipt_file.sha256.eq_ignore_ascii_case(&owned.sha256)
        {
            bail!("VC runtime receipt file does not match this build");
        }
        if !matches(&resolve_owned_path(&root, &owned.path)?, &owned)? {
            bail!("installed VC runtime failed integrity verification");
        }
    }
    validate_exact_tree(&root, delivery.files)
}

fn validate_exact_tree(root: &Path, files: &[VcRuntimeFile]) -> Result<()> {
    let mut expected = files
        .iter()
        .map(|file| PathBuf::from(file.path))
        .collect::<HashSet<_>>();
    expected.insert(PathBuf::from(RECEIPT_NAME));
    let actual = crate::component_registry::owned_tree::regular_files(
        root,
        "VC runtime",
        MAX_COMPONENT_FILES + 1,
    )?
    .into_iter()
    .collect::<HashSet<_>>();
    if actual != expected {
        bail!("VC runtime contains unowned files");
    }
    Ok(())
}

fn owned_file(file: &VcRuntimeFile) -> OwnedComponentFile {
    OwnedComponentFile {
        path: PathBuf::from(file.path),
        size_bytes: file.size_bytes,
        sha256: file.sha256.to_string(),
    }
}

fn lock_component_files(root: &Path, files: &[VcRuntimeFile]) -> Result<Vec<std::fs::File>> {
    files
        .iter()
        .map(|expected| {
            super::native_file::lock_verified(
                root,
                expected.path,
                expected.size_bytes,
                expected.sha256,
                "VC runtime changed while acquiring its use lease",
            )
        })
        .collect()
}

#[cfg(all(test, not(feature = "recorder-worker")))]
mod tests;
