//! Isolated ownership boundary for the optional Qwen3 CUDA runtime.

use std::collections::HashSet;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

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

mod archive;
mod install;

const COMPONENT_ID: &str = "qwen3-cuda-runtime";
const VC_COMPONENT_ID: &str = "vc14-x64-runtime";
const ARCHITECTURE: &str = "x64";
const DISPLAY_NAME: &str = "Qwen3 CUDA runtime";
const MAX_COMPONENT_FILES: usize = 64;

#[derive(Clone, Copy)]
struct QwenRuntimeFile {
    archive_index: usize,
    archive_path: &'static str,
    path: &'static str,
    size_bytes: u64,
    sha256: &'static str,
}

#[derive(Clone, Copy)]
struct QwenRuntimeArchive {
    url: &'static str,
    size_bytes: u64,
    sha256: &'static str,
}

struct QwenRuntimeDelivery {
    version: &'static str,
    archives: &'static [QwenRuntimeArchive],
    unpacked_size_bytes: u64,
    files: &'static [QwenRuntimeFile],
}

include!(concat!(env!("OUT_DIR"), "/qwen_runtime_delivery.rs"));

pub(crate) struct QwenRuntimeUse {
    bin_dir: PathBuf,
    _files: Vec<std::fs::File>,
    _lease: Option<ComponentLease>,
    vc_runtime: super::vc_runtime::VcRuntimeUse,
}

impl QwenRuntimeUse {
    pub(crate) fn preload_dependencies(self) -> Result<QwenRuntimeLoadUse> {
        let Self {
            bin_dir,
            _files,
            _lease,
            vc_runtime,
        } = self;
        Ok(QwenRuntimeLoadUse {
            bin_dir,
            _files,
            _lease,
            _vc_runtime: vc_runtime.preload()?,
        })
    }
}

pub(crate) struct QwenRuntimeLoadUse {
    bin_dir: PathBuf,
    _files: Vec<std::fs::File>,
    _lease: Option<ComponentLease>,
    _vc_runtime: super::vc_runtime::LoadedVcRuntime,
}

impl QwenRuntimeLoadUse {
    pub(crate) fn bin_dir(&self) -> &Path {
        &self.bin_dir
    }
}

pub(crate) fn ensure_component(
    cancel: &AtomicBool,
    on_progress: impl Fn(u64, u64),
) -> Result<QwenRuntimeUse> {
    let _mutation = super::acquire_mutation_guard()?;
    let vc_runtime = super::vc_runtime::ensure_component(|_, _| {})?;
    #[cfg(debug_assertions)]
    if let Some(bin_dir) = development_root() {
        return Ok(QwenRuntimeUse {
            _files: lock_development_files(&bin_dir)?,
            bin_dir,
            _lease: None,
            vc_runtime,
        });
    }
    let delivery = delivery()?;
    if validate_install(delivery).is_err() {
        install::install(cancel, on_progress)?;
    }
    acquire_with_vc(delivery, vc_runtime)
}

pub(crate) fn acquire_installed() -> Result<QwenRuntimeUse> {
    let vc_runtime = super::vc_runtime::ensure_component(|_, _| {})?;
    #[cfg(debug_assertions)]
    if let Some(bin_dir) = development_root() {
        return Ok(QwenRuntimeUse {
            _files: lock_development_files(&bin_dir)?,
            bin_dir,
            _lease: None,
            vc_runtime,
        });
    }
    acquire_with_vc(delivery()?, vc_runtime)
}

fn acquire_with_vc(
    delivery: &QwenRuntimeDelivery,
    vc_runtime: super::vc_runtime::VcRuntimeUse,
) -> Result<QwenRuntimeUse> {
    let lease = super::acquire(COMPONENT_ID)?;
    let root = super::validate_version_root(COMPONENT_ID, delivery.version)?;
    let files = lock_component_files(&root, delivery.files)?;
    validate_install(delivery)?;
    Ok(QwenRuntimeUse {
        bin_dir: root.join("bin/x64"),
        _files: files,
        _lease: Some(lease),
        vc_runtime,
    })
}

pub(crate) fn is_installed() -> bool {
    #[cfg(debug_assertions)]
    if development_root().is_some() {
        return true;
    }
    QWEN_RUNTIME_DELIVERY
        .as_ref()
        .is_some_and(|delivery| validate_install(delivery).is_ok())
}

#[cfg(not(feature = "recorder-worker"))]
pub(crate) fn is_installed_for_display() -> bool {
    #[cfg(debug_assertions)]
    if development_root().is_some() {
        return true;
    }
    QWEN_RUNTIME_DELIVERY
        .as_ref()
        .is_some_and(|delivery| validate_status(delivery).is_ok())
}

pub(crate) fn active_bin_dir() -> Option<PathBuf> {
    #[cfg(debug_assertions)]
    if let Some(root) = development_root() {
        return Some(root);
    }
    let delivery = QWEN_RUNTIME_DELIVERY.as_ref()?;
    validate_install(delivery).ok()?;
    version_root(delivery).ok().map(|root| root.join("bin/x64"))
}

#[cfg(not(feature = "recorder-worker"))]
pub(crate) fn active_bin_dir_for_display() -> Option<PathBuf> {
    #[cfg(debug_assertions)]
    if let Some(root) = development_root() {
        return Some(root);
    }
    let delivery = QWEN_RUNTIME_DELIVERY.as_ref()?;
    validate_status(delivery).ok()?;
    version_root(delivery).ok().map(|root| root.join("bin/x64"))
}

#[cfg(not(feature = "recorder-worker"))]
pub(crate) fn installed_size() -> u64 {
    QWEN_RUNTIME_DELIVERY
        .as_ref()
        .filter(|delivery| validate_install(delivery).is_ok())
        .map_or(0, |delivery| delivery.unpacked_size_bytes)
}

#[cfg(not(feature = "recorder-worker"))]
pub(crate) fn installed_size_for_display() -> u64 {
    QWEN_RUNTIME_DELIVERY
        .as_ref()
        .filter(|delivery| validate_status(delivery).is_ok())
        .map_or(0, |delivery| delivery.unpacked_size_bytes)
}

#[cfg(not(feature = "recorder-worker"))]
pub(crate) fn remove() -> Result<()> {
    match super::request_remove(COMPONENT_ID)? {
        RemovalOutcome::Missing | RemovalOutcome::Removed | RemovalOutcome::Pending => Ok(()),
        RemovalOutcome::RequiredBy(dependents) => bail!(
            "{DISPLAY_NAME} is required by installed components: {}",
            dependents.join(", ")
        ),
        RemovalOutcome::PreservedModified(paths) => bail!(
            "{DISPLAY_NAME} contains {} modified managed file(s); they were preserved",
            paths.len()
        ),
    }
}

fn delivery() -> Result<&'static QwenRuntimeDelivery> {
    QWEN_RUNTIME_DELIVERY
        .as_ref()
        .ok_or_else(|| anyhow!("verified {DISPLAY_NAME} delivery is not included in this build"))
}

fn version_root(delivery: &QwenRuntimeDelivery) -> Result<PathBuf> {
    super::component_version_root(COMPONENT_ID, delivery.version)
}

fn validate_install(delivery: &QwenRuntimeDelivery) -> Result<()> {
    validate_receipt(delivery, file_matches)
}

#[cfg(not(feature = "recorder-worker"))]
fn validate_status(delivery: &QwenRuntimeDelivery) -> Result<()> {
    validate_receipt(delivery, file_size_matches)
}

fn validate_receipt(
    delivery: &QwenRuntimeDelivery,
    matcher: fn(&Path, &OwnedComponentFile) -> Result<bool>,
) -> Result<()> {
    if delivery
        .files
        .iter()
        .map(|file| file.size_bytes)
        .sum::<u64>()
        != delivery.unpacked_size_bytes
    {
        bail!("Qwen3 runtime delivery inventory size is inconsistent");
    }
    let root = super::validate_version_root(COMPONENT_ID, delivery.version)?;
    let receipt = ComponentReceipt::read(&root.join(RECEIPT_NAME))?;
    if receipt.id != COMPONENT_ID
        || receipt.version != delivery.version
        || receipt.architecture != ARCHITECTURE
        || receipt.dependencies != [VC_COMPONENT_ID]
        || receipt.files.len() != delivery.files.len()
    {
        bail!("Qwen3 runtime receipt does not match this build");
    }
    for (receipt_file, expected) in receipt.files.iter().zip(delivery.files) {
        let owned = owned_file(expected);
        if receipt_file.path != owned.path
            || receipt_file.size_bytes != owned.size_bytes
            || !receipt_file.sha256.eq_ignore_ascii_case(&owned.sha256)
            || !matcher(&resolve_owned_path(&root, &owned.path)?, &owned)?
        {
            bail!("installed Qwen3 runtime failed integrity verification");
        }
    }
    validate_exact_tree(&root, delivery.files)
}

fn validate_exact_tree(root: &Path, files: &[QwenRuntimeFile]) -> Result<()> {
    let mut expected = files
        .iter()
        .map(|file| PathBuf::from(file.path))
        .collect::<HashSet<_>>();
    expected.insert(PathBuf::from(RECEIPT_NAME));
    let mut actual = HashSet::new();
    collect_regular_files(root, root, &mut actual, 0)?;
    if actual != expected {
        bail!("Qwen3 runtime contains unowned files");
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
        bail!("Qwen3 runtime exceeds traversal limits");
    }
    let metadata = std::fs::symlink_metadata(directory)?;
    if !metadata.is_dir() || is_reparse_point(&metadata) {
        bail!("Qwen3 runtime contains an unsafe directory");
    }
    for entry in std::fs::read_dir(directory)? {
        let path = entry?.path();
        let metadata = std::fs::symlink_metadata(&path)?;
        if metadata.is_dir() && !is_reparse_point(&metadata) {
            collect_regular_files(root, &path, files, depth + 1)?;
        } else if metadata.is_file() && !is_reparse_point(&metadata) {
            files.insert(path.strip_prefix(root)?.to_path_buf());
        } else {
            bail!("Qwen3 runtime contains an unsafe entry");
        }
    }
    Ok(())
}

fn owned_file(file: &QwenRuntimeFile) -> OwnedComponentFile {
    OwnedComponentFile {
        path: PathBuf::from(file.path),
        size_bytes: file.size_bytes,
        sha256: file.sha256.to_string(),
    }
}

fn lock_component_files(root: &Path, files: &[QwenRuntimeFile]) -> Result<Vec<std::fs::File>> {
    let mut locked = Vec::with_capacity(files.len());
    for expected in files {
        let path = resolve_owned_path(root, Path::new(expected.path))?;
        let mut file = open_locked_regular_file(&path)?;
        let metadata = file.metadata()?;
        if metadata.len() != expected.size_bytes {
            bail!("Qwen3 runtime changed while acquiring its load lease");
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
            bail!("Qwen3 runtime changed while acquiring its load lease");
        }
        locked.push(file);
    }
    Ok(locked)
}

fn open_locked_regular_file(path: &Path) -> Result<std::fs::File> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_file() || is_reparse_point(&metadata) {
        bail!("Qwen3 runtime load file is unsafe");
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    use std::os::windows::fs::OpenOptionsExt as _;
    use windows::Win32::Storage::FileSystem::FILE_SHARE_READ;
    options.share_mode(FILE_SHARE_READ.0);
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || is_reparse_point(&metadata) {
        bail!("Qwen3 runtime load file is unsafe");
    }
    Ok(file)
}

#[cfg(debug_assertions)]
fn lock_development_files(root: &Path) -> Result<Vec<std::fs::File>> {
    [
        "sgt_qwen3_runtime.dll",
        "c10.dll",
        "c10_cuda.dll",
        "torch_cpu.dll",
        "torch_cuda.dll",
    ]
    .iter()
    .map(|name| open_locked_regular_file(&root.join(name)))
    .collect()
}

#[cfg(debug_assertions)]
fn development_root() -> Option<PathBuf> {
    let root = std::env::var_os("SGT_QWEN3_RUNTIME_DEV_DIR").map(PathBuf::from)?;
    let runtime = root.join("sgt_qwen3_runtime.dll");
    archive::validate_x64_pe(&runtime).ok()?;
    ["c10.dll", "c10_cuda.dll", "torch_cpu.dll", "torch_cuda.dll"]
        .iter()
        .all(|name| root.join(name).is_file())
        .then_some(root)
}

#[cfg(all(test, not(feature = "recorder-worker")))]
mod tests;
