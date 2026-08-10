//! Exact, removable delivery and process-lifetime guards for Windows external tools.

use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{LazyLock, Mutex, MutexGuard};

use anyhow::{Result, anyhow, bail};
use sha2::{Digest, Sha256};

use super::receipt::{
    ComponentReceipt, OwnedComponentFile, RECEIPT_NAME, is_reparse_point, resolve_owned_path,
};
use super::{ComponentLease, RemovalOutcome};

mod install;
mod recovery;
mod recovery_io;
mod staging;

pub(crate) use recovery::{ExternalToolRecovery, RecoveryCleanupOutcome};

const ARCHITECTURE: &str = "x64";
const MAX_COMPONENT_FILES: usize = 8;
static TOOL_MUTATION_LOCKS: LazyLock<[Mutex<()>; 3]> =
    LazyLock::new(|| std::array::from_fn(|_| Mutex::new(())));

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ExternalTool {
    YtDlp,
    Ffmpeg,
    Deno,
}

impl ExternalTool {
    pub(crate) const ALL: [Self; 3] = [Self::YtDlp, Self::Ffmpeg, Self::Deno];

    pub(crate) const fn id(self) -> &'static str {
        match self {
            Self::YtDlp => "yt-dlp-x64",
            Self::Ffmpeg => "ffmpeg-x64",
            Self::Deno => "deno-x64",
        }
    }

    const fn executable(self) -> &'static str {
        match self {
            Self::YtDlp => "bin/x64/yt-dlp.exe",
            Self::Ffmpeg => "bin/x64/ffmpeg.exe",
            Self::Deno => "bin/x64/deno.exe",
        }
    }
}

#[derive(Clone, Copy)]
enum ExternalArchiveFormat {
    Raw,
    Zip,
}

const SUPPORTED_ARCHIVE_FORMATS: [ExternalArchiveFormat; 2] =
    [ExternalArchiveFormat::Raw, ExternalArchiveFormat::Zip];

#[derive(Clone, Copy)]
struct ExternalToolFile {
    path: &'static str,
    archive_path: &'static str,
    size_bytes: u64,
    sha256: &'static str,
}

struct ExternalToolDelivery {
    id: &'static str,
    version: &'static str,
    asset: &'static str,
    download_url: &'static str,
    archive_format: ExternalArchiveFormat,
    size_bytes: u64,
    sha256: &'static str,
    unpacked_size_bytes: u64,
    files: &'static [ExternalToolFile],
}

pub(crate) struct WebView2BootstrapperDelivery {
    pub(crate) version: &'static str,
    pub(crate) asset: &'static str,
    pub(crate) download_url: &'static str,
    pub(crate) size_bytes: u64,
    pub(crate) sha256: &'static str,
    pub(crate) expected_publisher: &'static str,
}

include!(concat!(env!("OUT_DIR"), "/external_tool_delivery.rs"));

#[derive(Clone, Debug)]
pub(crate) enum ExternalToolStatus {
    Installed { bytes: u64 },
    Missing,
    Unavailable,
    Error(String),
}

pub(crate) struct ExternalToolUse {
    tool: ExternalTool,
    root: PathBuf,
    _files: Vec<std::fs::File>,
    _lease: ComponentLease,
}

impl ExternalToolUse {
    pub(crate) fn executable(&self) -> PathBuf {
        self.root.join(self.tool.executable())
    }

    pub(crate) fn bin_dir(&self) -> PathBuf {
        self.root.join("bin/x64")
    }
}

pub(crate) fn ensure(
    tool: ExternalTool,
    cancelled: &AtomicBool,
    on_progress: impl Fn(u64, u64),
) -> Result<ExternalToolUse> {
    let delivery = delivery(tool)?;
    // External operations intentionally coalesce in-process callers before
    // taking the cross-process registry mutex. Generic removal never takes
    // this local lock, so the local -> global order cannot form a cycle.
    let _local = lock_tool_mutation(tool);
    let _global = super::acquire_mutation_guard()?;
    install::ensure(tool, delivery, cancelled, on_progress)
}

pub(crate) fn acquire_installed(tool: ExternalTool) -> Result<ExternalToolUse> {
    acquire_delivery(tool, delivery(tool)?)
}

pub(crate) fn current_status(tool: ExternalTool) -> ExternalToolStatus {
    let Some(delivery) = delivery_optional(tool) else {
        return ExternalToolStatus::Unavailable;
    };
    match validate_install_fast(delivery) {
        Ok(()) => ExternalToolStatus::Installed {
            bytes: delivery.unpacked_size_bytes,
        },
        Err(error) if version_root(delivery).is_ok_and(|root| root.exists()) => {
            ExternalToolStatus::Error(error.to_string())
        }
        Err(_) => ExternalToolStatus::Missing,
    }
}

pub(crate) fn version_label(tool: ExternalTool) -> Option<String> {
    delivery_optional(tool).map(|delivery| format!("{} (x64)", delivery.version))
}

pub(crate) fn remove(tool: ExternalTool) -> Result<RemovalOutcome> {
    super::request_remove(tool.id())
}

pub(crate) fn recoveries(tool: ExternalTool) -> Result<Vec<ExternalToolRecovery>> {
    recovery::list(tool)
}

pub(crate) fn clean_recovery(
    tool: ExternalTool,
    recovery: &ExternalToolRecovery,
) -> Result<RecoveryCleanupOutcome> {
    let _local = lock_tool_mutation(tool);
    let _global = super::acquire_mutation_guard()?;
    recovery::clean(tool, recovery)
}

pub(crate) fn clean_all_recoveries() -> Result<Vec<RecoveryCleanupOutcome>> {
    let _yt_dlp = lock_tool_mutation(ExternalTool::YtDlp);
    let _ffmpeg = lock_tool_mutation(ExternalTool::Ffmpeg);
    let _deno = lock_tool_mutation(ExternalTool::Deno);
    let _global = super::acquire_mutation_guard()?;
    recovery::clean_all()
}

pub(crate) fn reconcile_interrupted_installs() -> Result<()> {
    let _yt_dlp = lock_tool_mutation(ExternalTool::YtDlp);
    let _ffmpeg = lock_tool_mutation(ExternalTool::Ffmpeg);
    let _deno = lock_tool_mutation(ExternalTool::Deno);
    let _global = super::acquire_mutation_guard()?;
    install::reconcile_interrupted()
}

fn lock_tool_mutation(tool: ExternalTool) -> MutexGuard<'static, ()> {
    TOOL_MUTATION_LOCKS[tool_index(tool)]
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

const fn tool_index(tool: ExternalTool) -> usize {
    match tool {
        ExternalTool::YtDlp => 0,
        ExternalTool::Ffmpeg => 1,
        ExternalTool::Deno => 2,
    }
}

pub(crate) fn webview2_bootstrapper_delivery() -> Result<&'static WebView2BootstrapperDelivery> {
    WEBVIEW2_BOOTSTRAPPER_DELIVERY
        .as_ref()
        .ok_or_else(|| anyhow!("verified WebView2 bootstrapper delivery is not included"))
}

fn acquire_delivery(
    tool: ExternalTool,
    delivery: &'static ExternalToolDelivery,
) -> Result<ExternalToolUse> {
    validate_install_fast(delivery)?;
    let lease = super::acquire(delivery.id)?;
    let root = std::fs::canonicalize(version_root(delivery)?)?;
    let files = lock_component_files(&root, delivery.files)?;
    validate_install_fast(delivery)?;
    Ok(ExternalToolUse {
        tool,
        root,
        _files: files,
        _lease: lease,
    })
}

fn delivery(tool: ExternalTool) -> Result<&'static ExternalToolDelivery> {
    delivery_optional(tool).ok_or_else(|| {
        anyhow!(
            "verified {} delivery is not included in this build",
            tool.id()
        )
    })
}

fn delivery_optional(tool: ExternalTool) -> Option<&'static ExternalToolDelivery> {
    debug_assert_eq!(SUPPORTED_ARCHIVE_FORMATS.len(), 2);
    EXTERNAL_TOOL_DELIVERIES
        .iter()
        .find(|delivery| delivery.id == tool.id())
}

fn version_root(delivery: &ExternalToolDelivery) -> Result<PathBuf> {
    super::component_version_root(delivery.id, delivery.version)
}

fn validate_install_fast(delivery: &ExternalToolDelivery) -> Result<()> {
    let root = super::validate_version_root(delivery.id, delivery.version)?;
    let receipt = ComponentReceipt::read(&root.join(RECEIPT_NAME))?;
    if receipt.id != delivery.id
        || receipt.version != delivery.version
        || receipt.architecture != ARCHITECTURE
        || !receipt.dependencies.is_empty()
        || receipt.files.len() != delivery.files.len()
    {
        bail!("external tool receipt does not match this build");
    }
    for (actual, expected) in receipt.files.iter().zip(delivery.files) {
        let owned = owned_file(expected);
        if !same_file(actual, &owned)
            || !file_metadata_matches(&resolve_owned_path(&root, &owned.path)?, &owned)?
        {
            bail!("external tool failed integrity verification");
        }
    }
    validate_exact_tree(&root, delivery.files)
}

fn file_metadata_matches(path: &Path, expected: &OwnedComponentFile) -> Result<bool> {
    let metadata = std::fs::symlink_metadata(path)?;
    Ok(metadata.is_file() && !is_reparse_point(&metadata) && metadata.len() == expected.size_bytes)
}

fn validate_exact_tree(root: &Path, files: &[ExternalToolFile]) -> Result<()> {
    let mut actual = Vec::new();
    staging::collect_regular_files(root, root, &mut actual, MAX_COMPONENT_FILES + 1)?;
    actual.retain(|path| path != Path::new(RECEIPT_NAME));
    if actual.len() != files.len()
        || actual
            .iter()
            .any(|path| !files.iter().any(|file| Path::new(file.path) == path))
    {
        bail!("external tool contains unowned files");
    }
    Ok(())
}

fn owned_file(file: &ExternalToolFile) -> OwnedComponentFile {
    OwnedComponentFile {
        path: file.path.into(),
        size_bytes: file.size_bytes,
        sha256: file.sha256.to_string(),
    }
}

fn same_file(left: &OwnedComponentFile, right: &OwnedComponentFile) -> bool {
    left.path == right.path
        && left.size_bytes == right.size_bytes
        && left.sha256.eq_ignore_ascii_case(&right.sha256)
}

fn lock_component_files(root: &Path, files: &[ExternalToolFile]) -> Result<Vec<std::fs::File>> {
    #[cfg(test)]
    RUNTIME_HASH_PASSES.with(|passes| passes.set(passes.get() + 1));
    let mut locked = Vec::with_capacity(files.len());
    for expected in files {
        let path = resolve_owned_path(root, Path::new(expected.path))?;
        let mut file = open_locked_regular_file(&path)?;
        if file.metadata()?.len() != expected.size_bytes {
            bail!("external tool changed while acquiring its use lease");
        }
        let mut hasher = Sha256::new();
        let mut buffer = [0_u8; 128 * 1024];
        loop {
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
        }
        if !format!("{:x}", hasher.finalize()).eq_ignore_ascii_case(expected.sha256) {
            bail!("external tool changed while acquiring its use lease");
        }
        locked.push(file);
    }
    Ok(locked)
}

#[cfg(test)]
thread_local! {
    static RUNTIME_HASH_PASSES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
fn reset_runtime_hash_passes() {
    RUNTIME_HASH_PASSES.with(|passes| passes.set(0));
}

#[cfg(test)]
fn runtime_hash_passes() -> usize {
    RUNTIME_HASH_PASSES.with(std::cell::Cell::get)
}

fn open_locked_regular_file(path: &Path) -> Result<std::fs::File> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_file() || is_reparse_point(&metadata) {
        bail!("external tool use file is unsafe");
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;
        use windows::Win32::Storage::FileSystem::{FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ};
        options
            .share_mode(FILE_SHARE_READ.0)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0);
    }
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || is_reparse_point(&metadata) {
        bail!("external tool use file is unsafe");
    }
    Ok(file)
}

fn receipt(delivery: &ExternalToolDelivery) -> ComponentReceipt {
    ComponentReceipt {
        schema_version: 1,
        id: delivery.id.to_string(),
        version: delivery.version.to_string(),
        architecture: ARCHITECTURE.to_string(),
        dependencies: Vec::new(),
        files: delivery.files.iter().map(owned_file).collect(),
    }
}

#[cfg(test)]
mod tests;
