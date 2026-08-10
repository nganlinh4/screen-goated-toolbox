//! Verified delivery foundation for the shared Windows x64 VC support runtime.

use std::collections::HashSet;
use std::io::Read as _;
use std::path::{Path, PathBuf};
#[cfg(not(feature = "recorder-worker"))]
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
#[cfg(not(feature = "recorder-worker"))]
use std::sync::{LazyLock, Mutex};

use anyhow::{Result, anyhow, bail};

use super::ComponentLease;
#[cfg(not(feature = "recorder-worker"))]
use super::RemovalOutcome;
#[cfg(not(feature = "recorder-worker"))]
use super::receipt::file_size_matches;
use super::receipt::{
    ComponentReceipt, OwnedComponentFile, RECEIPT_NAME, file_matches, is_reparse_point,
    resolve_owned_path,
};

mod install;
mod staging;

const COMPONENT_ID: &str = "vc14-x64-runtime";
const ARCHITECTURE: &str = "x64";
const DISPLAY_NAME: &str = "Microsoft VC runtime support";
#[cfg(all(debug_assertions, not(feature = "recorder-worker")))]
const DEVELOPMENT_VERSION: &str = "14.50.35719.0";
const MAX_COMPONENT_FILES: usize = 16;

#[cfg(not(feature = "recorder-worker"))]
#[derive(Clone, Debug)]
pub(crate) enum VcRuntimeStatus {
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

#[cfg(debug_assertions)]
const DEVELOPMENT_FILES: &[VcRuntimeFile] = &[
    VcRuntimeFile {
        path: "bin/x64/concrt140.dll",
        size_bytes: 321_696,
        sha256: "b2faf3b85b23c840b654e57d5497a0ad31acd02fb01856cad4725a1715d5f78e",
    },
    VcRuntimeFile {
        path: "bin/x64/msvcp140.dll",
        size_bytes: 553_552,
        sha256: "def46aa6a8f72f27bafac0c43334419486a4d1dcdb6c479a8ef7034b3e1fa4cb",
    },
    VcRuntimeFile {
        path: "bin/x64/msvcp140_1.dll",
        size_bytes: 35_488,
        sha256: "2dd670f874562fbdca5b022df1943d70a57ba91fde559280e3a1daebe4db2380",
    },
    VcRuntimeFile {
        path: "bin/x64/msvcp140_2.dll",
        size_bytes: 278_608,
        sha256: "1d60da3ac2b06482912ca852fa7047436e6e474b4cfffa3bf77f4598cfbf454c",
    },
    VcRuntimeFile {
        path: "bin/x64/msvcp140_atomic_wait.dll",
        size_bytes: 48_800,
        sha256: "e7963645e0d1db08e300614d4c5fa7194bd8173e9ab7a5558859e6b232ed3241",
    },
    VcRuntimeFile {
        path: "bin/x64/msvcp140_codecvt_ids.dll",
        size_bytes: 31_392,
        sha256: "ae8d922b00cdd93e3ebecc37beb46c800f383ebdeb9f9e5b84e04a72428b6fb3",
    },
    VcRuntimeFile {
        path: "bin/x64/vccorlib140.dll",
        size_bytes: 350_880,
        sha256: "6b8d8a76c3e6664293407553650e60b94df9aaafc7c92057ea83032bd228e44f",
    },
    VcRuntimeFile {
        path: "bin/x64/vcruntime140.dll",
        size_bytes: 123_472,
        sha256: "184146852727a9db4eea06178716bec3cdbb1015c911f6b0f915b184ad7775b2",
    },
    VcRuntimeFile {
        path: "bin/x64/vcruntime140_1.dll",
        size_bytes: 47_264,
        sha256: "e6bfb3662ab4b1969a73441dbe35c96d51441b6bff8cf1fe7430bd5b246ca605",
    },
    VcRuntimeFile {
        path: "bin/x64/vcruntime140_threads.dll",
        size_bytes: 37_456,
        sha256: "a6222020b500a9a86b36e040c2dbd0e459716db1bf2810a11cd7512ea9b8d89b",
    },
];

#[cfg(not(feature = "recorder-worker"))]
static INSTALLING: AtomicBool = AtomicBool::new(false);
#[cfg(not(feature = "recorder-worker"))]
static PROGRESS_BASIS_POINTS: AtomicU32 = AtomicU32::new(0);
#[cfg(not(feature = "recorder-worker"))]
static LAST_NOTICE: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));

pub(crate) struct VcRuntimeUse {
    bin_dir: PathBuf,
    _files: Vec<std::fs::File>,
    _lease: Option<ComponentLease>,
}

impl VcRuntimeUse {
    pub(crate) fn bin_dir(&self) -> &Path {
        &self.bin_dir
    }

    pub(crate) fn preload(self) -> Result<LoadedVcRuntime> {
        const LOAD_ORDER: &[&str] = &[
            "vcruntime140.dll",
            "vcruntime140_1.dll",
            "vcruntime140_threads.dll",
            "msvcp140.dll",
            "msvcp140_1.dll",
            "msvcp140_2.dll",
            "msvcp140_atomic_wait.dll",
            "msvcp140_codecvt_ids.dll",
            "concrt140.dll",
            "vccorlib140.dll",
        ];
        let mut libraries = Vec::with_capacity(LOAD_ORDER.len());
        for name in LOAD_ORDER {
            let path = self.bin_dir.join(name);
            #[cfg(target_os = "windows")]
            let library = unsafe {
                use libloading::os::windows::{
                    LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR, LOAD_LIBRARY_SEARCH_SYSTEM32,
                };
                libloading::os::windows::Library::load_with_flags(
                    &path,
                    LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32,
                )
                .map(libloading::Library::from)
            }
            .map_err(|error| anyhow!("failed to load VC support '{}': {error}", path.display()))?;
            #[cfg(not(target_os = "windows"))]
            let library = unsafe { libloading::Library::new(&path) }.map_err(|error| {
                anyhow!("failed to load VC support '{}': {error}", path.display())
            })?;
            libraries.push(library);
        }
        Ok(LoadedVcRuntime {
            _runtime: self,
            _libraries: libraries,
        })
    }
}

pub(crate) struct LoadedVcRuntime {
    _runtime: VcRuntimeUse,
    _libraries: Vec<libloading::Library>,
}

pub(crate) fn ensure_component(on_progress: impl Fn(u64, u64)) -> Result<VcRuntimeUse> {
    #[cfg(debug_assertions)]
    if let Some(bin_dir) = development_root() {
        return Ok(VcRuntimeUse {
            _files: lock_development_files(&bin_dir)?,
            bin_dir,
            _lease: None,
        });
    }

    let _mutation = super::acquire_mutation_guard()?;
    let delivery = delivery()?;
    if validate_install(delivery).is_err() {
        install::install(on_progress)?;
    }
    let lease = super::acquire(COMPONENT_ID)?;
    let root = version_root(delivery)?;
    let files = lock_component_files(&root, delivery.files)?;
    validate_install(delivery)?;
    Ok(VcRuntimeUse {
        bin_dir: root.join("bin/x64"),
        _files: files,
        _lease: Some(lease),
    })
}

#[cfg(not(feature = "recorder-worker"))]
pub(crate) fn current_status() -> VcRuntimeStatus {
    if INSTALLING.load(Ordering::Acquire) {
        return VcRuntimeStatus::Installing {
            progress: PROGRESS_BASIS_POINTS.load(Ordering::Relaxed) as f32 / 100.0,
        };
    }

    #[cfg(debug_assertions)]
    if development_root_for_status().is_some() {
        return VcRuntimeStatus::Development {
            bytes: development_bytes(),
        };
    }

    let Some(delivery) = VC_RUNTIME_DELIVERY.as_ref() else {
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
    #[cfg(debug_assertions)]
    if development_root_for_status().is_some() {
        return Some(format!("{DEVELOPMENT_VERSION} ({ARCHITECTURE})"));
    }
    VC_RUNTIME_DELIVERY
        .as_ref()
        .map(|delivery| format!("{} ({ARCHITECTURE})", delivery.version))
}

#[cfg(not(feature = "recorder-worker"))]
pub(crate) fn start_install() -> bool {
    let status = current_status();
    #[cfg(debug_assertions)]
    if matches!(status, VcRuntimeStatus::Development { .. }) {
        return false;
    }
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
    std::thread::spawn(|| {
        let language = crate::APP
            .lock()
            .map(|app| app.config.ui_language.clone())
            .unwrap_or_else(|_| "en".to_string());
        let component_name = crate::gui::locale::LocaleText::get(&language)
            .auxiliary
            .managed_tools
            .tool_vc_runtime
            .to_string();
        let badge = crate::overlay::auto_copy_badge::DownloadProgressBadge::new(&component_name);
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
    });
    true
}

#[cfg(not(feature = "recorder-worker"))]
pub(crate) fn remove() -> Result<()> {
    let result = match super::request_remove(COMPONENT_ID)? {
        RemovalOutcome::Missing | RemovalOutcome::Removed | RemovalOutcome::Pending => Ok(()),
        RemovalOutcome::RequiredBy(dependents) => bail!(
            "{DISPLAY_NAME} is required by installed components: {}",
            dependents.join(", ")
        ),
        RemovalOutcome::PreservedModified(paths) => bail!(
            "{DISPLAY_NAME} contains {} modified managed file(s); they were preserved",
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
    VC_RUNTIME_DELIVERY
        .as_ref()
        .ok_or_else(|| anyhow!("verified {DISPLAY_NAME} delivery is not included in this build"))
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
    let mut actual = HashSet::new();
    collect_regular_files(root, root, &mut actual, 0)?;
    if actual != expected {
        bail!("VC runtime contains unowned files");
    }
    Ok(())
}

fn collect_regular_files(
    root: &Path,
    directory: &Path,
    files: &mut HashSet<PathBuf>,
    depth: usize,
) -> Result<()> {
    if depth > 4 || files.len() > MAX_COMPONENT_FILES {
        bail!("VC runtime exceeds traversal limits");
    }
    let metadata = std::fs::symlink_metadata(directory)?;
    if !metadata.is_dir() || is_reparse_point(&metadata) {
        bail!("VC runtime contains an unsafe directory");
    }
    for entry in std::fs::read_dir(directory)? {
        let path = entry?.path();
        let metadata = std::fs::symlink_metadata(&path)?;
        if metadata.is_dir() && !is_reparse_point(&metadata) {
            collect_regular_files(root, &path, files, depth + 1)?;
        } else if metadata.is_file() && !is_reparse_point(&metadata) {
            files.insert(path.strip_prefix(root)?.to_path_buf());
        } else {
            bail!("VC runtime contains an unsafe entry");
        }
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
    let mut locked = Vec::with_capacity(files.len());
    for expected in files {
        let path = resolve_owned_path(root, Path::new(expected.path))?;
        let mut file = open_locked_regular_file(&path)?;
        if file.metadata()?.len() != expected.size_bytes {
            bail!("VC runtime changed while acquiring its use lease");
        }
        let mut hasher = sha2::Sha256::new();
        let mut buffer = [0_u8; 64 * 1024];
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
            bail!("VC runtime changed while acquiring its use lease");
        }
        locked.push(file);
    }
    Ok(locked)
}

#[cfg(debug_assertions)]
fn lock_development_files(root: &Path) -> Result<Vec<std::fs::File>> {
    let mut locked = Vec::with_capacity(DEVELOPMENT_FILES.len());
    for expected in DEVELOPMENT_FILES {
        let name = Path::new(expected.path)
            .file_name()
            .ok_or_else(|| anyhow!("VC runtime development file name is invalid"))?;
        let path = root.join(name);
        let mut file = open_locked_regular_file(&path)?;
        if file.metadata()?.len() != expected.size_bytes {
            bail!("VC runtime development file changed while acquiring its use lease");
        }
        let mut hasher = sha2::Sha256::new();
        let mut buffer = [0_u8; 64 * 1024];
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
            bail!("VC runtime development file changed while acquiring its use lease");
        }
        locked.push(file);
    }
    Ok(locked)
}

fn open_locked_regular_file(path: &Path) -> Result<std::fs::File> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_file() || is_reparse_point(&metadata) {
        bail!("VC runtime use file is unsafe");
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    use std::os::windows::fs::OpenOptionsExt as _;
    use windows::Win32::Storage::FileSystem::FILE_SHARE_READ;
    options.share_mode(FILE_SHARE_READ.0);
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || is_reparse_point(&metadata) {
        bail!("VC runtime use file is unsafe");
    }
    Ok(file)
}

#[cfg(all(debug_assertions, not(feature = "recorder-worker")))]
fn development_bytes() -> u64 {
    DEVELOPMENT_FILES.iter().map(|file| file.size_bytes).sum()
}

#[cfg(debug_assertions)]
fn development_root() -> Option<PathBuf> {
    development_root_matching(file_matches)
}

#[cfg(all(debug_assertions, not(feature = "recorder-worker")))]
fn development_root_for_status() -> Option<PathBuf> {
    development_root_matching(file_size_matches)
}

#[cfg(debug_assertions)]
fn development_root_matching(
    matches: fn(&Path, &OwnedComponentFile) -> Result<bool>,
) -> Option<PathBuf> {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/embed_dlls/x64");
    DEVELOPMENT_FILES
        .iter()
        .all(|file| {
            Path::new(file.path)
                .file_name()
                .is_some_and(|name| matches(&source.join(name), &owned_file(file)).unwrap_or(false))
        })
        .then_some(source)
}

#[cfg(all(test, not(feature = "recorder-worker")))]
mod tests;
