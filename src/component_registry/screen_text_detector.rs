//! Verified PaddleOCR detector worker/model delivery and active-use ownership.

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
mod update;

pub(crate) const ID: &str = "screen-text-detector";
const ARCHITECTURE: &str = "x64";
const RUNTIME_ID: &str = "onnx-directml-runtime";
const EXECUTABLE_PATH: &str = "bin/x64/sgt-screen-text-detector-worker.exe";
const MODEL_DIR: &str = "models/pp-ocr-screen-text";
const MAX_COMPONENT_FILES: usize = 28;
const MAX_COMPONENT_ENTRIES: usize = 64;

struct DetectorFile {
    path: &'static str,
    size_bytes: u64,
    sha256: &'static str,
}

struct DetectorDelivery {
    version: &'static str,
    asset: &'static str,
    download_url: &'static str,
    size_bytes: u64,
    sha256: &'static str,
    unpacked_size_bytes: u64,
    files: &'static [DetectorFile],
}

include!(concat!(
    env!("OUT_DIR"),
    "/screen_text_detector_delivery.rs"
));

pub(crate) struct DetectorUse {
    executable: PathBuf,
    model_dir: PathBuf,
    _lease: ComponentLease,
    _files: Vec<std::fs::File>,
}

impl DetectorUse {
    pub(crate) fn executable(&self) -> &Path {
        &self.executable
    }

    pub(crate) fn model_dir(&self) -> &Path {
        &self.model_dir
    }
}

pub(crate) fn ensure(
    cancelled: &AtomicBool,
    on_progress: impl Fn(u64, u64),
) -> Result<DetectorUse> {
    super::update_catalog::refresh_for_use(ID, "before-session");
    let _mutation = super::acquire_mutation_guard()?;
    let delivery = delivery()?;
    install::ensure(delivery, cancelled, on_progress)?;
    let lease = super::acquire(ID)?;
    let root = version_root(delivery)?;
    let files = lock_component_files(&root, delivery.files)?;
    validate_install(delivery)?;
    let executable = resolve_owned_path(&root, Path::new(EXECUTABLE_PATH))?;
    install::validate_x64_pe(&executable)?;
    Ok(DetectorUse {
        executable,
        model_dir: root.join(MODEL_DIR),
        _lease: lease,
        _files: files,
    })
}

pub(crate) fn is_installed() -> bool {
    delivery().is_ok_and(|delivery| validate_status(delivery).is_ok())
}

pub(crate) fn delivery_available() -> bool {
    delivery().is_ok()
}

pub(crate) fn localized_name() -> String {
    let language = crate::APP
        .lock()
        .map(|app| app.config.ui_language.clone())
        .unwrap_or_else(|_| "en".to_string());
    crate::gui::locale::LocaleText::get(&language)
        .auxiliary
        .managed_tools
        .tool_screen_translate_detector
        .to_string()
}

pub(crate) fn installed_size() -> u64 {
    delivery()
        .ok()
        .filter(|delivery| validate_status(delivery).is_ok())
        .map(|delivery| delivery.unpacked_size_bytes)
        .unwrap_or(0)
}

pub(crate) fn remove() -> Result<()> {
    crate::overlay::screen_translate::stop_detector();
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

fn delivery() -> Result<&'static DetectorDelivery> {
    if let Some(delivery) = update::delivery() {
        return Ok(delivery);
    }
    DETECTOR_DELIVERY
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Screen Translate detector contract is unavailable"))
}

fn version_root(delivery: &DetectorDelivery) -> Result<PathBuf> {
    super::component_version_root(ID, delivery.version)
}

fn validate_install(delivery: &DetectorDelivery) -> Result<()> {
    validate_receipt(delivery, file_matches)
}

fn validate_status(delivery: &DetectorDelivery) -> Result<()> {
    validate_receipt(delivery, file_size_matches)
}

fn validate_receipt(
    delivery: &DetectorDelivery,
    matches: fn(&Path, &OwnedComponentFile) -> Result<bool>,
) -> Result<()> {
    let root = super::validate_version_root(ID, delivery.version)?;
    let receipt = ComponentReceipt::read(&root.join(RECEIPT_NAME))?;
    if receipt.id != ID
        || receipt.version != delivery.version
        || receipt.architecture != ARCHITECTURE
        || receipt.dependencies != [RUNTIME_ID]
        || receipt.files.len() != delivery.files.len()
    {
        bail!("Screen Translate detector receipt does not match this build");
    }
    for expected in delivery.files {
        let owned = owned_file(expected);
        if !receipt.files.iter().any(|entry| same_file(entry, &owned)) {
            bail!("Screen Translate detector receipt inventory does not match this build");
        }
        let path = resolve_owned_path(&root, Path::new(expected.path))?;
        if !matches(&path, &owned)? {
            bail!("Screen Translate detector failed integrity verification");
        }
    }
    validate_exact_tree(&root, delivery.files, true)
}

fn validate_exact_tree(root: &Path, files: &[DetectorFile], include_receipt: bool) -> Result<()> {
    let actual = super::staging::collect_tree(root, MAX_COMPONENT_ENTRIES)?;
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
        bail!("Screen Translate detector tree does not match its inventory");
    }
    Ok(())
}

fn owned_file(file: &DetectorFile) -> OwnedComponentFile {
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

fn lock_component_files(root: &Path, files: &[DetectorFile]) -> Result<Vec<std::fs::File>> {
    let mut locked = Vec::with_capacity(files.len());
    for expected in files {
        let path = resolve_owned_path(root, Path::new(expected.path))?;
        let mut file = open_locked_regular_file(&path)?;
        if file.metadata()?.len() != expected.size_bytes {
            bail!("Screen Translate detector changed while acquiring its lease");
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
            bail!("Screen Translate detector changed while acquiring its lease");
        }
        locked.push(file);
    }
    Ok(locked)
}

fn open_locked_regular_file(path: &Path) -> Result<std::fs::File> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_file() || is_reparse_point(&metadata) {
        bail!("Screen Translate detector launch file is unsafe");
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    use std::os::windows::fs::OpenOptionsExt as _;
    use windows::Win32::Storage::FileSystem::{FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ};
    options
        .share_mode(FILE_SHARE_READ.0)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0);
    Ok(options.open(path)?)
}

fn receipt(delivery: &DetectorDelivery) -> ComponentReceipt {
    ComponentReceipt {
        schema_version: 1,
        id: ID.to_string(),
        version: delivery.version.to_string(),
        architecture: ARCHITECTURE.to_string(),
        dependencies: vec![RUNTIME_ID.to_string()],
        files: delivery.files.iter().map(owned_file).collect(),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn tracked_delivery_is_present() {
        assert!(super::delivery_available());
    }
}
