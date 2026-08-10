//! Downloadable Windows x64 ASR worker and ONNX/DirectML dependency.

use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{LazyLock, Mutex};

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
mod validation;

#[cfg(debug_assertions)]
use validation::validate_runtime_install;
use validation::{
    owned_file, validate_delivery, validate_delivery_status, validate_exact_tree,
    validate_runtime_status,
};

pub(crate) const WORKER_ID: &str = "local-asr-worker";
pub(crate) const RUNTIME_ID: &str = "onnx-directml-runtime";
const ARCHITECTURE: &str = "x64";
const VC_RUNTIME_ID: &str = "vc14-x64-runtime";
const WORKER_FILE: &str = "bin/x64/sgt-local-asr-worker.exe";
const RUNTIME_VERSION: &str = "1.24.2-directml-1.15.4";
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

const RUNTIME_FILES: &[LocalAsrFile] = &[
    LocalAsrFile {
        path: "bin/x64/onnxruntime.dll",
        size_bytes: 17_270_304,
        sha256: "a2323bc49544645b911743052f1edce594e17df1e3423b71468c7386bc902f80",
    },
    LocalAsrFile {
        path: "bin/x64/onnxruntime_providers_shared.dll",
        size_bytes: 22_048,
        sha256: "8b33b30ac866c938aa3d946d4f92fc2ba70fff06ef45d5ce22e483f19ba2c896",
    },
    LocalAsrFile {
        path: "bin/x64/DirectML.dll",
        size_bytes: 18_527_776,
        sha256: "9c9e6d822561c6c41b90e6994b3e8857cf1d66dbfb1e0c4c799c7c89b4e92da1",
    },
    LocalAsrFile {
        path: "licenses/onnxruntime-LICENSE.txt",
        size_bytes: 1_094,
        sha256: "c250d6278f0b47a6439fb7592b08b58a55eb9f535aa49a1db63211c3f982b674",
    },
    LocalAsrFile {
        path: "licenses/onnxruntime-ThirdPartyNotices.txt",
        size_bytes: 331_175,
        sha256: "fb0af774b4d7cffc5b9d046f2aaeade2f37df2f80abf8033c95dfffcc77a8866",
    },
    LocalAsrFile {
        path: "licenses/directml-LICENSE-CODE.txt",
        size_bytes: 1_093,
        sha256: "903df5512f7d02609fed0c780a9b704f5a3eeb6e4d84ebe42a29845c81899a3c",
    },
    LocalAsrFile {
        path: "licenses/directml-LICENSE.txt",
        size_bytes: 10_439,
        sha256: "a05138e3a085ff60a44881eedfa58dccb03ecc1d7b1f6ae888418e8c2fec4b8d",
    },
    LocalAsrFile {
        path: "licenses/directml-ThirdPartyNotices.txt",
        size_bytes: 4_577,
        sha256: "2c95795c13ff48a58b6ed916f37901c23d964b5d9d601af422f17ad2172e7950",
    },
];

#[cfg(not(feature = "recorder-worker"))]
#[derive(Clone, Debug)]
pub(crate) enum ComponentStatus {
    #[cfg(debug_assertions)]
    Development {
        bytes: u64,
    },
    Installed {
        bytes: u64,
        version: String,
    },
    Installing {
        progress: f32,
    },
    Missing,
    Unavailable,
    Error(String),
}

#[cfg(feature = "recorder-worker")]
pub(crate) fn status_is_ready(status: &ComponentStatus) -> bool {
    match status {
        ComponentStatus::Installed => true,
        #[cfg(debug_assertions)]
        ComponentStatus::Development => true,
        _ => false,
    }
}

#[cfg(feature = "recorder-worker")]
#[derive(Clone, Copy, Debug)]
pub(crate) enum ComponentStatus {
    #[cfg(debug_assertions)]
    Development,
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
    _lease: Option<ComponentLease>,
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
    #[cfg(debug_assertions)]
    if let Some(executable) = development_worker() {
        return Ok(LocalAsrWorkerUse {
            _files: vec![open_locked_regular_file(&executable)?],
            executable,
            _lease: None,
        });
    }
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
        _lease: Some(lease),
    })
}

pub(crate) fn ensure_runtime(
    cancelled: &AtomicBool,
    on_progress: impl Fn(u64, u64),
) -> Result<OnnxRuntimeUse> {
    let _mutation = super::acquire_mutation_guard()?;
    let root = if let Some(delivery) = optional_delivery(RUNTIME_ID) {
        install::ensure_delivery(delivery, cancelled, on_progress)?;
        validate_delivery(delivery)?;
        version_root(RUNTIME_ID, delivery.version)?
    } else {
        #[cfg(debug_assertions)]
        {
            install::ensure_development_runtime(cancelled, on_progress)?;
            validate_runtime_install()?;
            version_root(RUNTIME_ID, RUNTIME_VERSION)?
        }
        #[cfg(not(debug_assertions))]
        bail!("the local ONNX/DirectML runtime is not included in this build")
    };
    let lease = super::acquire(RUNTIME_ID)?;
    let files = lock_component_files(&root, RUNTIME_FILES)?;
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
    #[cfg(debug_assertions)]
    if kind == ComponentKind::Worker
        && let Some(_executable) = development_worker_for_status()
    {
        #[cfg(feature = "recorder-worker")]
        return ComponentStatus::Development;
        #[cfg(not(feature = "recorder-worker"))]
        return ComponentStatus::Development {
            bytes: _executable.metadata().map(|value| value.len()).unwrap_or(0),
        };
    }
    #[cfg(not(debug_assertions))]
    if kind == ComponentKind::Runtime && optional_delivery(RUNTIME_ID).is_none() {
        return ComponentStatus::Unavailable;
    }
    if kind == ComponentKind::Runtime && validate_runtime_status().is_ok() {
        #[cfg(feature = "recorder-worker")]
        return ComponentStatus::Installed;
        #[cfg(not(feature = "recorder-worker"))]
        return ComponentStatus::Installed {
            bytes: RUNTIME_FILES.iter().map(|file| file.size_bytes).sum(),
            version: RUNTIME_VERSION.to_string(),
        };
    }
    let Some(delivery) = optional_delivery(kind.id()) else {
        return ComponentStatus::Unavailable;
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

#[cfg(not(feature = "recorder-worker"))]
pub(crate) fn current_notice(kind: ComponentKind) -> Option<String> {
    notice(kind)
        .lock()
        .unwrap_or_else(|value| value.into_inner())
        .clone()
}

#[cfg(not(feature = "recorder-worker"))]
pub(crate) fn version_label(kind: ComponentKind) -> Option<String> {
    if kind == ComponentKind::Runtime {
        return Some(format!(
            "ONNX Runtime 1.24.2 + DirectML 1.15.4 ({ARCHITECTURE})"
        ));
    }
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
    #[cfg(all(not(feature = "recorder-worker"), debug_assertions))]
    let ready = ready || matches!(status, ComponentStatus::Development { .. });
    if ready
        || installing
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
    {
        return false;
    }
    clear_notice(kind);
    std::thread::spawn(move || {
        let cancelled = AtomicBool::new(false);
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
    match super::request_remove(kind.id())? {
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
            "{} contains {} modified managed file(s); they were preserved",
            kind.id(),
            paths.len()
        ),
    }
}

fn delivery(id: &str) -> Result<&'static LocalAsrDelivery> {
    optional_delivery(id).ok_or_else(|| anyhow!("{id} is not included in this build"))
}

fn optional_delivery(id: &str) -> Option<&'static LocalAsrDelivery> {
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

#[cfg(debug_assertions)]
fn development_worker_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("native/local_asr_worker/target/x86_64-pc-windows-msvc/debug")
        .join("sgt-local-asr-worker.exe")
}

#[cfg(debug_assertions)]
fn development_worker_for_status() -> Option<PathBuf> {
    let path = development_worker_path();
    let metadata = std::fs::symlink_metadata(&path).ok()?;
    (metadata.is_file() && !is_reparse_point(&metadata) && metadata.len() > 0).then_some(path)
}

#[cfg(debug_assertions)]
fn development_worker() -> Option<PathBuf> {
    let path = development_worker_path();
    install::validate_x64_pe(&path).is_ok().then_some(path)
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
