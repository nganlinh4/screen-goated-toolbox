//! Verified delivery and active-use ownership for Computer Control's data-only engine.

use std::collections::HashSet;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use anyhow::{Result, bail};
use sha2::{Digest as _, Sha256};

use super::receipt::{
    ComponentReceipt, OwnedComponentFile, RECEIPT_NAME, file_matches, file_size_matches,
    is_reparse_point, resolve_owned_path,
};
use super::{ComponentLease, RemovalOutcome};

mod install;
mod staging;

pub(crate) const ID: &str = "computer-control-engine";
const ARCHITECTURE: &str = "x64";
const EXECUTABLE_PATH: &str = "bin/x64/sgt-computer-control-engine.exe";
const MAX_COMPONENT_FILES: usize = 16;
const MAX_COMPONENT_ENTRIES: usize = 64;

struct EngineFile {
    path: &'static str,
    size_bytes: u64,
    sha256: &'static str,
}

struct EngineDelivery {
    version: &'static str,
    asset: &'static str,
    download_url: &'static str,
    size_bytes: u64,
    sha256: &'static str,
    unpacked_size_bytes: u64,
    files: &'static [EngineFile],
}

include!(concat!(env!("OUT_DIR"), "/computer_control_delivery.rs"));

pub(crate) struct ComputerControlEngineUse {
    executable: PathBuf,
    _lease: Option<ComponentLease>,
    _files: Vec<std::fs::File>,
}

impl ComputerControlEngineUse {
    pub(crate) fn executable(&self) -> &Path {
        &self.executable
    }
}

pub(crate) fn ensure_engine(
    cancelled: &AtomicBool,
    on_progress: impl Fn(u64, u64),
) -> Result<ComputerControlEngineUse> {
    #[cfg(debug_assertions)]
    if ENGINE_DELIVERY.is_none() {
        return development_engine();
    }

    let _mutation = super::acquire_mutation_guard()?;
    let delivery = delivery()?;
    install::ensure(delivery, cancelled, on_progress)?;
    let lease = super::acquire(ID)?;
    let root = version_root(delivery)?;
    let files = lock_component_files(&root, delivery.files)?;
    validate_install(delivery)?;
    let executable = resolve_owned_path(&root, Path::new(EXECUTABLE_PATH))?;
    validate_x64_pe(&executable)?;
    Ok(ComputerControlEngineUse {
        executable,
        _lease: Some(lease),
        _files: files,
    })
}

pub(crate) fn ensure_engine_with_badge(cancelled: &AtomicBool) -> Result<ComputerControlEngineUse> {
    let badge = crate::overlay::auto_copy_badge::DownloadProgressBadge::new(&component_name());
    let result = ensure_engine(cancelled, |done, total| badge.report(done, total));
    badge.finish();
    result
}

pub(crate) fn download_title() -> String {
    let locale = crate::overlay::auto_copy_badge::locale_text();
    crate::overlay::auto_copy_badge::format_locale(
        locale.downloading_component_fmt,
        &[("name", &component_name())],
    )
}

pub(crate) fn is_installed() -> bool {
    delivery().is_ok_and(|delivery| validate_status(delivery).is_ok())
}

pub(crate) fn installed_size() -> u64 {
    delivery()
        .ok()
        .filter(|delivery| validate_status(delivery).is_ok())
        .map(|delivery| delivery.unpacked_size_bytes)
        .unwrap_or(0)
}

pub(crate) fn remove() -> Result<()> {
    match super::request_remove(ID)? {
        RemovalOutcome::Missing | RemovalOutcome::Removed | RemovalOutcome::Pending => Ok(()),
        RemovalOutcome::PreservedModified(paths) => bail!(
            "{ID} contains {} modified managed file(s); they were preserved",
            paths.len()
        ),
        RemovalOutcome::RequiredBy(dependents) => {
            bail!("{ID} is required by {}", dependents.join(", "))
        }
    }
}

pub(crate) fn download_from_manager(
    stop: std::sync::Arc<AtomicBool>,
    use_badge: bool,
) -> Result<()> {
    let badge = use_badge
        .then(|| crate::overlay::auto_copy_badge::DownloadProgressBadge::new(&component_name()));
    set_download_state(0.0);
    let result = ensure_engine(&stop, |done, total| {
        let progress = done as f32 / total.max(1) as f32 * 100.0;
        set_download_state(progress);
        if let Some(badge) = &badge {
            badge.report(done, total);
        }
    })
    .map(drop);
    if let Ok(mut state) = crate::overlay::realtime_webview::state::REALTIME_STATE.lock() {
        state.is_downloading = false;
        state.download_progress = if result.is_ok() { 100.0 } else { 0.0 };
    }
    if let Some(badge) = &badge {
        badge.finish();
    }
    if let Err(error) = &result {
        let locale = crate::overlay::auto_copy_badge::locale_text();
        let title = crate::overlay::auto_copy_badge::format_locale(
            locale.component_install_failed_fmt,
            &[("name", &component_name())],
        );
        crate::overlay::auto_copy_badge::show_detailed_notification(
            &title,
            &error.to_string(),
            crate::overlay::auto_copy_badge::NotificationType::Error,
        );
    }
    result
}

fn set_download_state(progress: f32) {
    let title = download_title();
    let message = component_message();
    if let Ok(mut state) = crate::overlay::realtime_webview::state::REALTIME_STATE.lock() {
        state.is_downloading = true;
        state.download_title = title;
        state.download_message = message;
        state.download_progress = progress;
    }
}

fn component_name() -> String {
    current_locale()
        .auxiliary
        .managed_tools
        .tool_computer_control_card
        .to_string()
}

fn component_message() -> String {
    current_locale()
        .auxiliary
        .managed_tools
        .tool_computer_control_payload
        .to_string()
}

fn current_locale() -> crate::gui::locale::LocaleText {
    let language = crate::APP
        .lock()
        .map(|app| app.config.ui_language.clone())
        .unwrap_or_else(|_| "en".to_string());
    crate::gui::locale::LocaleText::get(&language)
}

fn delivery() -> Result<&'static EngineDelivery> {
    ENGINE_DELIVERY
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Computer Control engine delivery is not included"))
}

fn version_root(delivery: &EngineDelivery) -> Result<PathBuf> {
    super::component_version_root(ID, delivery.version)
}

fn validate_install(delivery: &EngineDelivery) -> Result<()> {
    validate_receipt(delivery, file_matches)
}

fn validate_status(delivery: &EngineDelivery) -> Result<()> {
    validate_receipt(delivery, file_size_matches)
}

fn validate_receipt(
    delivery: &EngineDelivery,
    matches: fn(&Path, &OwnedComponentFile) -> Result<bool>,
) -> Result<()> {
    let root = super::validate_version_root(ID, delivery.version)?;
    let receipt = ComponentReceipt::read(&root.join(RECEIPT_NAME))?;
    if receipt.id != ID
        || receipt.version != delivery.version
        || receipt.architecture != ARCHITECTURE
        || !receipt.dependencies.is_empty()
        || receipt.files.len() != delivery.files.len()
    {
        bail!("Computer Control engine receipt does not match this build");
    }
    for expected in delivery.files {
        let owned = owned_file(expected);
        if !receipt.files.iter().any(|entry| same_file(entry, &owned)) {
            bail!("Computer Control engine receipt inventory does not match this build");
        }
        let path = resolve_owned_path(&root, Path::new(expected.path))?;
        if !matches(&path, &owned)? {
            bail!("Computer Control engine failed integrity verification");
        }
    }
    validate_exact_tree(&root, delivery.files, true)
}

fn validate_exact_tree(root: &Path, files: &[EngineFile], include_receipt: bool) -> Result<()> {
    let actual = staging::collect_tree(root, MAX_COMPONENT_ENTRIES)?;
    let mut expected_files = files
        .iter()
        .map(|file| PathBuf::from(file.path))
        .collect::<HashSet<_>>();
    if include_receipt {
        expected_files.insert(PathBuf::from(RECEIPT_NAME));
    }
    let mut expected_directories = HashSet::new();
    for file in files {
        let mut parent = Path::new(file.path).parent();
        while let Some(directory) = parent {
            if directory.as_os_str().is_empty() {
                break;
            }
            expected_directories.insert(directory.to_path_buf());
            parent = directory.parent();
        }
    }
    if actual.files.into_iter().collect::<HashSet<_>>() != expected_files
        || actual.directories.into_iter().collect::<HashSet<_>>() != expected_directories
    {
        bail!("Computer Control engine tree does not match its exact inventory");
    }
    Ok(())
}

fn owned_file(file: &EngineFile) -> OwnedComponentFile {
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

fn lock_component_files(root: &Path, files: &[EngineFile]) -> Result<Vec<std::fs::File>> {
    let mut locked = Vec::with_capacity(files.len());
    for expected in files {
        let path = resolve_owned_path(root, Path::new(expected.path))?;
        let mut file = open_locked_regular_file(&path)?;
        if file.metadata()?.len() != expected.size_bytes {
            bail!("Computer Control engine changed while acquiring its lease");
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
            bail!("Computer Control engine changed while acquiring its lease");
        }
        locked.push(file);
    }
    Ok(locked)
}

fn open_locked_regular_file(path: &Path) -> Result<std::fs::File> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_file() || is_reparse_point(&metadata) {
        bail!("Computer Control engine launch file is unsafe");
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    use std::os::windows::fs::OpenOptionsExt as _;
    use windows::Win32::Storage::FileSystem::{FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ};
    options
        .share_mode(FILE_SHARE_READ.0)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0);
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || is_reparse_point(&metadata) {
        bail!("Computer Control engine launch file is unsafe");
    }
    Ok(file)
}

fn validate_x64_pe(path: &Path) -> Result<()> {
    use std::io::{Read as _, Seek as _, SeekFrom};
    let mut file = std::fs::File::open(path)?;
    let mut dos = [0_u8; 64];
    file.read_exact(&mut dos)?;
    if &dos[..2] != b"MZ" {
        bail!("Computer Control engine is not a PE executable");
    }
    let offset = u32::from_le_bytes(dos[0x3c..0x40].try_into().unwrap());
    file.seek(SeekFrom::Start(offset as u64))?;
    let mut header = [0_u8; 6];
    file.read_exact(&mut header)?;
    if &header[..4] != b"PE\0\0" || u16::from_le_bytes([header[4], header[5]]) != 0x8664 {
        bail!("Computer Control engine is not an x64 PE executable");
    }
    Ok(())
}

#[cfg(debug_assertions)]
fn development_engine() -> Result<ComputerControlEngineUse> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("native/computer_control_engine/target");
    let explicit_target = root
        .join("x86_64-pc-windows-msvc/debug")
        .join("sgt-computer-control-engine.exe");
    let default_target = root.join("debug").join("sgt-computer-control-engine.exe");
    let executable = if explicit_target.is_file() {
        explicit_target
    } else {
        default_target
    };
    validate_x64_pe(&executable)?;
    Ok(ComputerControlEngineUse {
        executable,
        _lease: None,
        _files: Vec::new(),
    })
}

fn receipt(delivery: &EngineDelivery) -> ComponentReceipt {
    ComponentReceipt {
        schema_version: 1,
        id: ID.to_string(),
        version: delivery.version.to_string(),
        architecture: ARCHITECTURE.to_string(),
        dependencies: Vec::new(),
        files: delivery.files.iter().map(owned_file).collect(),
    }
}
