//! Verified delivery and launch leases for the external Screen Recorder.

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
mod update;

pub(crate) const WEB_ID: &str = "recorder-web";
pub(crate) const WORKER_ID: &str = "recorder-worker";
const ARCHITECTURE: &str = "x64";
const WORKER_PATH: &str = "bin/x64/sgt-recorder-worker.exe";
const MAX_COMPONENT_FILES: usize = 512;

struct RecorderFile {
    path: &'static str,
    size_bytes: u64,
    sha256: &'static str,
}

struct RecorderDelivery {
    id: &'static str,
    version: &'static str,
    asset: &'static str,
    download_url: &'static str,
    size_bytes: u64,
    sha256: &'static str,
    unpacked_size_bytes: u64,
    files: &'static [RecorderFile],
}

include!(concat!(env!("OUT_DIR"), "/recorder_delivery.rs"));

pub(crate) struct RecorderComponents {
    pub(crate) web_root: PathBuf,
    pub(crate) worker_path: PathBuf,
    _leases: Vec<ComponentLease>,
    _files: Vec<std::fs::File>,
}

pub(crate) fn ensure_ready(
    cancelled: &AtomicBool,
    on_progress: impl Fn(u64, u64),
) -> Result<RecorderComponents> {
    super::update_catalog::refresh_for_use(WEB_ID, "before-open");
    let _mutation = super::acquire_mutation_guard()?;
    let web = delivery(WEB_ID)?;
    let worker = delivery(WORKER_ID)?;
    install::ensure_pair(web, worker, cancelled, on_progress)?;

    // Reserve both components before hashing. Removal cannot begin after these
    // leases are acquired, and the file handles below prevent write/delete
    // races for every byte in both exact inventories.
    let web_lease = super::acquire(WEB_ID)?;
    let worker_lease = match super::acquire(WORKER_ID) {
        Ok(lease) => lease,
        Err(error) => {
            drop(web_lease);
            return Err(error);
        }
    };
    let web_root = version_root(web)?;
    let worker_root = version_root(worker)?;
    let mut files = lock_component_files(&web_root, web.files)?;
    match lock_component_files(&worker_root, worker.files) {
        Ok(mut worker_files) => files.append(&mut worker_files),
        Err(error) => {
            drop(worker_lease);
            drop(web_lease);
            return Err(error);
        }
    }
    validate_install(web)?;
    validate_install(worker)?;
    let worker_path = resolve_owned_path(&worker_root, Path::new(WORKER_PATH))?;
    validate_x64_pe(&worker_path)?;
    Ok(RecorderComponents {
        web_root,
        worker_path,
        _leases: vec![web_lease, worker_lease],
        _files: files,
    })
}

pub(crate) fn ensure_ready_with_badge(cancelled: &AtomicBool) -> Result<RecorderComponents> {
    let badge = crate::overlay::auto_copy_badge::DownloadProgressBadge::new(&component_name());
    let result = ensure_ready(cancelled, |done, total| badge.report(done, total));
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
    let (Ok(web), Ok(worker)) = (delivery(WEB_ID), delivery(WORKER_ID)) else {
        return false;
    };
    validate_status(web).is_ok() && validate_status(worker).is_ok()
}

pub(crate) fn delivery_available() -> bool {
    delivery(WEB_ID).is_ok() && delivery(WORKER_ID).is_ok()
}

pub(crate) fn installed_size() -> u64 {
    [WEB_ID, WORKER_ID]
        .into_iter()
        .filter_map(|id| delivery(id).ok())
        .filter(|entry| validate_status(entry).is_ok())
        .map(|entry| entry.unpacked_size_bytes)
        .sum()
}

pub(crate) fn remove_all() -> Result<()> {
    let _worker_shutdown = crate::overlay::screen_record::stop_for_component_removal()?;
    remove_one(WORKER_ID)?;
    remove_one(WEB_ID)
}

pub(crate) fn remove_from_manager() -> Result<()> {
    let name = component_name();
    let locale = crate::overlay::auto_copy_badge::locale_text();
    let removing = crate::overlay::auto_copy_badge::format_locale(
        locale.removing_component_fmt,
        &[("name", &name)],
    );
    crate::overlay::auto_copy_badge::show_detailed_notification(
        &removing,
        "",
        crate::overlay::auto_copy_badge::NotificationType::Info,
    );

    let result = remove_all();
    let locale = crate::overlay::auto_copy_badge::locale_text();
    let (template, kind, detail) = match &result {
        Ok(()) => (
            locale.component_removed_fmt,
            crate::overlay::auto_copy_badge::NotificationType::Success,
            String::new(),
        ),
        Err(error) => (
            locale.component_remove_failed_fmt,
            crate::overlay::auto_copy_badge::NotificationType::Error,
            format!("{error:#}"),
        ),
    };
    let title = crate::overlay::auto_copy_badge::format_locale(template, &[("name", &name)]);
    crate::overlay::auto_copy_badge::show_detailed_notification(&title, &detail, kind);
    result
}

pub(crate) fn download_from_manager(
    stop: std::sync::Arc<AtomicBool>,
    use_badge: bool,
) -> Result<()> {
    let badge = use_badge
        .then(|| crate::overlay::auto_copy_badge::DownloadProgressBadge::new(&component_name()));
    set_download_state(0.0);
    let result = ensure_ready(&stop, |done, total| {
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
        .tool_screen_recorder_card
        .to_string()
}

fn component_message() -> String {
    current_locale()
        .auxiliary
        .managed_tools
        .tool_screen_recorder_payload
        .to_string()
}

fn current_locale() -> crate::gui::locale::LocaleText {
    let language = crate::APP
        .lock()
        .map(|app| app.config.ui_language.clone())
        .unwrap_or_else(|_| "en".to_string());
    crate::gui::locale::LocaleText::get(&language)
}

fn remove_one(id: &str) -> Result<()> {
    match super::request_remove(id)? {
        RemovalOutcome::Missing | RemovalOutcome::Removed => Ok(()),
        RemovalOutcome::Pending => bail!("{id} is still in use after recorder shutdown"),
        RemovalOutcome::PreservedModified(paths) => bail!(
            "{id} contains {} modified managed file(s); they were preserved",
            paths.len()
        ),
        RemovalOutcome::RequiredBy(dependents) => {
            bail!(
                "{id} is required by installed components: {}",
                dependents.join(", ")
            )
        }
    }
}

fn delivery(id: &str) -> Result<&'static RecorderDelivery> {
    if let Some(delivery) = update::deliveries()
        .iter()
        .find(|delivery| delivery.id == id)
    {
        return Ok(delivery);
    }
    RECORDER_DELIVERIES
        .iter()
        .find(|entry| entry.id == id)
        .ok_or_else(|| anyhow::anyhow!("Screen Recorder download contract is unavailable"))
}

fn version_root(delivery: &RecorderDelivery) -> Result<PathBuf> {
    super::component_version_root(delivery.id, delivery.version)
}

fn dependencies(id: &str) -> Vec<String> {
    match id {
        WORKER_ID => vec![WEB_ID.to_string()],
        _ => Vec::new(),
    }
}

fn validate_install(delivery: &RecorderDelivery) -> Result<()> {
    validate_receipt(delivery, file_matches)
}

fn validate_status(delivery: &RecorderDelivery) -> Result<()> {
    validate_receipt(delivery, file_size_matches)
}

fn validate_receipt(
    delivery: &RecorderDelivery,
    matches: fn(&Path, &OwnedComponentFile) -> Result<bool>,
) -> Result<()> {
    let root = super::validate_version_root(delivery.id, delivery.version)?;
    let receipt = ComponentReceipt::read(&root.join(RECEIPT_NAME))?;
    if receipt.id != delivery.id
        || receipt.version != delivery.version
        || receipt.architecture != ARCHITECTURE
        || receipt.dependencies != dependencies(delivery.id)
        || receipt.files.len() != delivery.files.len()
    {
        bail!("recorder component receipt does not match this build");
    }
    for expected in delivery.files {
        let owned = owned_file(expected);
        if !receipt.files.iter().any(|entry| same_file(entry, &owned)) {
            bail!("recorder component receipt inventory does not match this build");
        }
        let path = resolve_owned_path(&root, Path::new(expected.path))?;
        if !matches(&path, &owned)? {
            bail!("recorder component failed integrity verification");
        }
    }
    validate_exact_tree(&root, delivery.files)
}

fn validate_exact_tree(root: &Path, files: &[RecorderFile]) -> Result<()> {
    let mut actual = Vec::new();
    staging::collect_regular_files(root, root, &mut actual, MAX_COMPONENT_FILES + 1)?;
    actual.retain(|path| path != Path::new(RECEIPT_NAME));
    if actual.len() != files.len()
        || actual
            .iter()
            .any(|path| !files.iter().any(|file| Path::new(file.path) == path))
    {
        bail!("recorder component contains unowned files");
    }
    Ok(())
}

fn owned_file(file: &RecorderFile) -> OwnedComponentFile {
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

fn lock_component_files(root: &Path, files: &[RecorderFile]) -> Result<Vec<std::fs::File>> {
    let mut locked = Vec::with_capacity(files.len());
    for expected in files {
        let path = resolve_owned_path(root, Path::new(expected.path))?;
        let mut file = open_locked_regular_file(&path)?;
        if file.metadata()?.len() != expected.size_bytes {
            bail!("recorder component changed while acquiring its launch lease");
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
            bail!("recorder component changed while acquiring its launch lease");
        }
        locked.push(file);
    }
    Ok(locked)
}

fn open_locked_regular_file(path: &Path) -> Result<std::fs::File> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_file() || is_reparse_point(&metadata) {
        bail!("recorder launch file is unsafe");
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    use std::os::windows::fs::OpenOptionsExt as _;
    use windows::Win32::Storage::FileSystem::FILE_SHARE_READ;
    options.share_mode(FILE_SHARE_READ.0);
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || is_reparse_point(&metadata) {
        bail!("recorder launch file is unsafe");
    }
    Ok(file)
}

fn validate_x64_pe(path: &Path) -> Result<()> {
    use std::io::{Read as _, Seek as _, SeekFrom};
    let mut file = std::fs::File::open(path)?;
    let mut dos = [0_u8; 64];
    file.read_exact(&mut dos)?;
    if &dos[..2] != b"MZ" {
        bail!("recorder worker is not a PE executable");
    }
    let offset = u32::from_le_bytes(dos[0x3c..0x40].try_into().unwrap());
    file.seek(SeekFrom::Start(offset as u64))?;
    let mut header = [0_u8; 6];
    file.read_exact(&mut header)?;
    if &header[..4] != b"PE\0\0" || u16::from_le_bytes([header[4], header[5]]) != 0x8664 {
        bail!("recorder worker is not an x64 PE executable");
    }
    Ok(())
}

fn receipt(delivery: &RecorderDelivery) -> ComponentReceipt {
    ComponentReceipt {
        schema_version: 1,
        id: delivery.id.to_string(),
        version: delivery.version.to_string(),
        architecture: ARCHITECTURE.to_string(),
        dependencies: dependencies(delivery.id),
        files: delivery.files.iter().map(owned_file).collect(),
    }
}

#[cfg(test)]
mod acceptance_tests {
    #[test]
    fn tracked_delivery_contains_recorder_web_and_worker() {
        assert!(super::delivery_available());
    }

    #[test]
    #[ignore = "opens the local recorder window"]
    fn active_recorder_is_stopped_before_removal_returns() {
        crate::overlay::screen_record::show_screen_record();

        super::remove_all().unwrap();

        assert!(!crate::overlay::screen_record::post_script(
            "void 0".to_string()
        ));
    }
}
