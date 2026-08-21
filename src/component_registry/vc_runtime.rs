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

#[cfg(all(test, not(feature = "recorder-worker")))]
mod tests;
