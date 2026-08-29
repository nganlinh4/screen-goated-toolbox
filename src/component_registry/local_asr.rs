//! Downloadable Windows x64 ASR worker and ONNX/DirectML dependency.

use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use anyhow::{Result, anyhow, bail};

use super::ComponentLease;
#[cfg(not(feature = "recorder-worker"))]
use super::RemovalOutcome;
use super::receipt::{
    ComponentReceipt, OwnedComponentFile, RECEIPT_NAME, file_matches, file_size_matches,
    is_reparse_point, resolve_owned_path,
};

mod install;
mod staging;
#[cfg(not(feature = "recorder-worker"))]
mod update;
mod validation;

use validation::{owned_file, validate_delivery, validate_delivery_status, validate_exact_tree};

pub(crate) const WORKER_ID: &str = "local-asr-worker";
pub(crate) const RUNTIME_ID: &str = "onnx-directml-runtime";
const ARCHITECTURE: &str = "x64";
const VC_RUNTIME_ID: &str = "vc14-x64-runtime";
const WORKER_FILE: &str = "bin/x64/sgt-local-asr-worker.exe";
const MAX_COMPONENT_FILES: usize = 8;

#[derive(Clone, Copy)]
struct LocalAsrFile {
    path: &'static str,
    size_bytes: u64,
    sha256: &'static str,
}

struct LocalAsrDelivery {
    id: &'static str,
    version: &'static str,
    download_url: &'static str,
    size_bytes: u64,
    sha256: &'static str,
    unpacked_size_bytes: u64,
    files: &'static [LocalAsrFile],
}

include!(concat!(env!("OUT_DIR"), "/local_asr_delivery.rs"));

#[cfg(not(feature = "recorder-worker"))]
#[derive(Clone, Debug)]
pub(crate) enum ComponentStatus {
    Installed { bytes: u64, version: String },
    Installing { progress: f32 },
    Missing,
    Unavailable,
    Error(String),
}

#[cfg(feature = "recorder-worker")]
pub(crate) fn status_is_ready(status: &ComponentStatus) -> bool {
    matches!(status, ComponentStatus::Installed)
}

#[cfg(feature = "recorder-worker")]
#[derive(Clone, Copy, Debug)]
pub(crate) enum ComponentStatus {
    Installed,
    Installing,
    Missing,
    Unavailable,
    Error,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ComponentKind {
    Worker,
    Runtime,
}

impl ComponentKind {
    fn id(self) -> &'static str {
        match self {
            Self::Worker => WORKER_ID,
            Self::Runtime => RUNTIME_ID,
        }
    }

    fn dependencies(self) -> Vec<String> {
        match self {
            Self::Worker => vec![RUNTIME_ID.to_string()],
            Self::Runtime => vec![VC_RUNTIME_ID.to_string()],
        }
    }
}

static INSTALLING_WORKER: AtomicBool = AtomicBool::new(false);
static INSTALLING_RUNTIME: AtomicBool = AtomicBool::new(false);
static WORKER_PROGRESS: AtomicU32 = AtomicU32::new(0);
static RUNTIME_PROGRESS: AtomicU32 = AtomicU32::new(0);
static WORKER_NOTICE: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));
static RUNTIME_NOTICE: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));

pub(crate) struct LocalAsrWorkerUse {
    executable: PathBuf,
    _files: Vec<std::fs::File>,
    _lease: ComponentLease,
}

impl LocalAsrWorkerUse {
    pub(crate) fn executable(&self) -> &Path {
        &self.executable
    }
}

pub(crate) struct OnnxRuntimeUse {
    bin_dir: PathBuf,
    _files: Vec<std::fs::File>,
    _lease: ComponentLease,
}

impl OnnxRuntimeUse {
    pub(crate) fn bin_dir(&self) -> &Path {
        &self.bin_dir
    }
}

pub(crate) fn ensure_worker(
    cancelled: &AtomicBool,
    on_progress: impl Fn(u64, u64),
) -> Result<LocalAsrWorkerUse> {
    #[cfg(not(feature = "recorder-worker"))]
    super::update_catalog::refresh_for_use(WORKER_ID, "before-session");
    let _mutation = super::acquire_mutation_guard()?;
    let delivery = delivery(WORKER_ID)?;
    install::ensure_delivery(delivery, cancelled, on_progress)?;
    let root = version_root(WORKER_ID, delivery.version)?;
    let lease = super::acquire(WORKER_ID)?;
    validate_delivery(delivery)?;
    let files = lock_component_files(&root, delivery.files)?;
    Ok(LocalAsrWorkerUse {
        executable: root.join(WORKER_FILE),
        _files: files,
        _lease: lease,
    })
}

pub(crate) fn ensure_runtime(
    cancelled: &AtomicBool,
    on_progress: impl Fn(u64, u64),
) -> Result<OnnxRuntimeUse> {
    let _mutation = super::acquire_mutation_guard()?;
    let delivery = delivery(RUNTIME_ID)?;
    install::ensure_delivery(delivery, cancelled, on_progress)?;
    validate_delivery(delivery)?;
    let root = version_root(RUNTIME_ID, delivery.version)?;
    let lease = super::acquire(RUNTIME_ID)?;
    let files = lock_component_files(&root, delivery.files)?;
    Ok(OnnxRuntimeUse {
        bin_dir: root.join("bin/x64"),
        _files: files,
        _lease: lease,
    })
}

pub(crate) fn current_status(kind: ComponentKind) -> ComponentStatus {
    let (installing, _progress) = progress_state(kind);
    if installing.load(Ordering::Acquire) {
        #[cfg(feature = "recorder-worker")]
        return ComponentStatus::Installing;
        #[cfg(not(feature = "recorder-worker"))]
        return ComponentStatus::Installing {
            progress: _progress.load(Ordering::Relaxed) as f32 / 100.0,
        };
    }
    if kind == ComponentKind::Runtime && optional_delivery(RUNTIME_ID).is_none() {
        return ComponentStatus::Unavailable;
    }
    let Some(delivery) = optional_delivery(kind.id()) else {
        return status_without_delivery(kind);
    };
    match validate_delivery_status(delivery) {
        #[cfg(feature = "recorder-worker")]
        Ok(()) => ComponentStatus::Installed,
        #[cfg(not(feature = "recorder-worker"))]
        Ok(()) => ComponentStatus::Installed {
            bytes: delivery.unpacked_size_bytes,
            version: delivery.version.to_string(),
        },
        #[cfg(feature = "recorder-worker")]
        Err(_) if version_root(delivery.id, delivery.version).is_ok_and(|root| root.exists()) => {
            ComponentStatus::Error
        }
        #[cfg(not(feature = "recorder-worker"))]
        Err(error)
            if version_root(delivery.id, delivery.version).is_ok_and(|root| root.exists()) =>
        {
            ComponentStatus::Error(error.to_string())
        }
        Err(_) => ComponentStatus::Missing,
    }
}

fn status_without_delivery(_kind: ComponentKind) -> ComponentStatus {
    ComponentStatus::Unavailable
}

#[cfg(not(feature = "recorder-worker"))]
pub(crate) fn current_notice(kind: ComponentKind) -> Option<String> {
    notice(kind)
        .lock()
        .unwrap_or_else(|value| value.into_inner())
        .clone()
}

#[cfg(not(feature = "recorder-worker"))]
pub(crate) fn version_label(kind: ComponentKind) -> Option<String> {
    optional_delivery(kind.id()).map(|delivery| format!("{} ({ARCHITECTURE})", delivery.version))
}

pub(crate) fn start_install(kind: ComponentKind) -> bool {
    let (installing, progress) = progress_state(kind);
    let status = current_status(kind);
    #[cfg(feature = "recorder-worker")]
    let ready = status_is_ready(&status) || matches!(status, ComponentStatus::Installing);
    #[cfg(not(feature = "recorder-worker"))]
    let ready = matches!(
        status,
        ComponentStatus::Installed { .. } | ComponentStatus::Installing { .. }
    );
    if ready
        || installing
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
    {
        return false;
    }
    clear_notice(kind);
    let cancelled = Arc::new(AtomicBool::new(false));
    let Ok(activity) = crate::install_activity::register(cancelled.clone()) else {
        installing.store(false, Ordering::Release);
        return false;
    };
    std::thread::spawn(move || {
        let _activity = activity;
        let badge = crate::overlay::auto_copy_badge::DownloadProgressBadge::new(
            &localized_component_name(kind),
        );
        let result = match kind {
            ComponentKind::Worker => (|| {
                let vc = super::vc_runtime::ensure_component(|done, total| {
                    badge.report(done.saturating_mul(10), total.saturating_mul(100));
                })?;
                let runtime = ensure_runtime(&cancelled, |done, total| {
                    let combined_done = total
                        .saturating_mul(10)
                        .saturating_add(done.saturating_mul(50));
                    let combined_total = total.saturating_mul(100);
                    store_progress(progress, combined_done, combined_total);
                    badge.report(combined_done, combined_total);
                })?;
                let worker = ensure_worker(&cancelled, |done, total| {
                    let combined_done = total
                        .saturating_mul(60)
                        .saturating_add(done.saturating_mul(40));
                    let combined_total = total.saturating_mul(100);
                    store_progress(progress, combined_done, combined_total);
                    badge.report(combined_done, combined_total);
                })?;
                drop((worker, runtime, vc));
                Ok(())
            })(),
            ComponentKind::Runtime => ensure_runtime(&cancelled, |done, total| {
                store_progress(progress, done, total);
                badge.report(done, total);
            })
            .map(drop),
        };
        if let Err(error) = result {
            *notice(kind)
                .lock()
                .unwrap_or_else(|value| value.into_inner()) = Some(error.to_string());
        }
        progress.store(0, Ordering::Relaxed);
        installing.store(false, Ordering::Release);
    });
    true
}

fn localized_component_name(kind: ComponentKind) -> String {
    let language = crate::APP
        .lock()
        .map(|app| app.config.ui_language.clone())
        .unwrap_or_else(|_| "en".to_string());
    let text = crate::gui::locale::LocaleText::get(&language);
    match kind {
        ComponentKind::Worker => text.auxiliary.managed_tools.tool_local_asr_worker,
        ComponentKind::Runtime => text.auxiliary.managed_tools.tool_onnx_runtime,
    }
    .to_string()
}

#[cfg(not(feature = "recorder-worker"))]
pub(crate) fn remove(kind: ComponentKind) -> Result<()> {
    let _owners = crate::overlay::component_removal::stop_audio_owners()?;
    match super::request_remove_and_wait(kind.id())? {
        RemovalOutcome::Missing | RemovalOutcome::Removed | RemovalOutcome::Pending => {
            clear_notice(kind);
            Ok(())
        }
        RemovalOutcome::RequiredBy(dependents) => bail!(
            "{} is required by installed components: {}",
            kind.id(),
            dependents.join(", ")
        ),
        RemovalOutcome::PreservedModified(paths) => bail!(
            "{} contains {} unrecorded or unsafe path(s); they were preserved",
            kind.id(),
            paths.len()
        ),
    }
}

fn delivery(id: &str) -> Result<&'static LocalAsrDelivery> {
    optional_delivery(id).ok_or_else(|| anyhow!("{id} download contract is unavailable"))
}

fn optional_delivery(id: &str) -> Option<&'static LocalAsrDelivery> {
    #[cfg(not(feature = "recorder-worker"))]
    if let Some(delivery) = update::deliveries()
        .iter()
        .find(|delivery| delivery.id == id)
    {
        return Some(delivery);
    }
    LOCAL_ASR_DELIVERIES
        .iter()
        .find(|delivery| delivery.id == id)
}

fn version_root(id: &str, version: &str) -> Result<PathBuf> {
    super::component_version_root(id, version)
}

fn lock_component_files(root: &Path, files: &[LocalAsrFile]) -> Result<Vec<std::fs::File>> {
    let mut locked = Vec::with_capacity(files.len());
    for expected in files {
        let path = resolve_owned_path(root, Path::new(expected.path))?;
        let mut file = open_locked_regular_file(&path)?;
        let metadata = file.metadata()?;
        if metadata.len() != expected.size_bytes {
            bail!("local ASR component changed while acquiring its launch lease");
        }
        let mut hasher = sha2::Sha256::new();
        let mut buffer = [0_u8; 128 * 1024];
        loop {
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            use sha2::Digest as _;
            hasher.update(&buffer[..read]);
        }
        use sha2::Digest as _;
        if !format!("{:x}", hasher.finalize()).eq_ignore_ascii_case(expected.sha256) {
            bail!("local ASR component changed while acquiring its launch lease");
        }
        locked.push(file);
    }
    Ok(locked)
}

fn open_locked_regular_file(path: &Path) -> Result<std::fs::File> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_file() || is_reparse_point(&metadata) {
        bail!("local ASR launch file is unsafe");
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    use std::os::windows::fs::OpenOptionsExt as _;
    use windows::Win32::Storage::FileSystem::FILE_SHARE_READ;
    options.share_mode(FILE_SHARE_READ.0);
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || is_reparse_point(&metadata) {
        bail!("local ASR launch file is unsafe");
    }
    Ok(file)
}

impl ComponentKind {
    fn from_id(id: &str) -> Result<Self> {
        match id {
            WORKER_ID => Ok(Self::Worker),
            RUNTIME_ID => Ok(Self::Runtime),
            _ => bail!("unsupported local ASR component"),
        }
    }
}

fn progress_state(kind: ComponentKind) -> (&'static AtomicBool, &'static AtomicU32) {
    match kind {
        ComponentKind::Worker => (&INSTALLING_WORKER, &WORKER_PROGRESS),
        ComponentKind::Runtime => (&INSTALLING_RUNTIME, &RUNTIME_PROGRESS),
    }
}

fn notice(kind: ComponentKind) -> &'static Mutex<Option<String>> {
    match kind {
        ComponentKind::Worker => &WORKER_NOTICE,
        ComponentKind::Runtime => &RUNTIME_NOTICE,
    }
}

fn clear_notice(kind: ComponentKind) {
    *notice(kind)
        .lock()
        .unwrap_or_else(|value| value.into_inner()) = None;
}

fn store_progress(progress: &AtomicU32, done: u64, total: u64) {
    let basis_points = done
        .saturating_mul(10_000)
        .checked_div(total.max(1))
        .unwrap_or(0);
    progress.store(basis_points.min(10_000) as u32, Ordering::Relaxed);
}

fn receipt(kind: ComponentKind, version: &str, files: &[LocalAsrFile]) -> ComponentReceipt {
    ComponentReceipt {
        schema_version: 1,
        id: kind.id().to_string(),
        version: version.to_string(),
        architecture: ARCHITECTURE.to_string(),
        dependencies: kind.dependencies(),
        files: files.iter().map(owned_file).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ComponentKind, ComponentStatus, RUNTIME_ID, WORKER_ID, optional_delivery,
        status_without_delivery,
    };

    #[test]
    fn tracked_delivery_contains_worker_and_runtime() {
        assert!(optional_delivery(WORKER_ID).is_some());
        assert!(optional_delivery(RUNTIME_ID).is_some());
    }

    #[test]
    fn missing_delivery_is_never_replaced_by_a_local_build_artifact() {
        assert!(matches!(
            status_without_delivery(ComponentKind::Runtime),
            ComponentStatus::Unavailable
        ));
        assert!(matches!(
            status_without_delivery(ComponentKind::Worker),
            ComponentStatus::Unavailable
        ));
    }
}
