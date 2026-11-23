use super::GalleryEvent;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
mod catalog;
mod cursor_backup;
mod registry_values;
mod scaled;
mod sources;
mod summary;
mod system;
pub(super) use catalog::CursorCollectionSpec;
use catalog::collections as catalog_collections;
use catalog::{CollectionSource, REGISTRY_FILE_MAP, REQUIRED_FILES};
use cursor_backup::CursorBackup;
use scaled::scaled_cursor_file_map_for_size;
use sources::{preload_missing_from_github, preload_missing_from_rar, preload_missing_from_zip};
use summary::invalidate_summary_cache;
#[cfg(target_os = "windows")]
use system::{refresh_cursor_settings, write_accessibility_cursor_size};

#[derive(Clone, Copy)]
pub(crate) struct PointerCollectionSummary {
    pub downloaded_count: usize,
    pub total_count: usize,
    pub downloading_count: usize,
    pub downloaded_bytes: u64,
}

const CURSOR_BASE_SIZE_MIN: u32 = 16;
const CURSOR_BASE_SIZE_MAX: u32 = 128;
const CURSOR_BASE_SIZE_DEFAULT: u32 = 32;
const ACCESSIBILITY_CURSOR_SIZE_MIN: u32 = 1;
const ACCESSIBILITY_CURSOR_SIZE_MAX: u32 = 15;

fn cache_root() -> PathBuf {
    crate::paths::app_runtime_local_data_dir().join("pointer-gallery")
}

fn backup_path() -> PathBuf {
    cache_root().join("original-cursor-backup.json")
}

fn downloading_ids() -> &'static Mutex<HashSet<String>> {
    static IDS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    IDS.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Exclusive ownership of one collection's download folder; released on drop.
struct DownloadClaim(&'static str);

impl Drop for DownloadClaim {
    fn drop(&mut self) {
        downloading_ids().lock().unwrap().remove(self.0);
        invalidate_summary_cache();
    }
}

fn claim_download(id: &'static str) -> Option<DownloadClaim> {
    {
        let mut ids = downloading_ids().lock().unwrap();
        if !ids.insert(id.to_string()) {
            return None;
        }
    }
    invalidate_summary_cache();
    Some(DownloadClaim(id))
}

fn is_downloading(id: &str) -> bool {
    downloading_ids().lock().unwrap().contains(id)
}

fn is_collection_complete(spec: CursorCollectionSpec, cache_root: &Path) -> bool {
    let dir = spec.local_dir(cache_root);
    REQUIRED_FILES.iter().all(|name| dir.join(name).exists())
}

fn clamp_cursor_base_size(size: u32) -> u32 {
    size.clamp(CURSOR_BASE_SIZE_MIN, CURSOR_BASE_SIZE_MAX)
}

pub(super) fn cursor_base_size_bounds() -> (u32, u32) {
    (CURSOR_BASE_SIZE_MIN, CURSOR_BASE_SIZE_MAX)
}

fn accessibility_cursor_size_from_base(base_size: u32) -> u32 {
    let clamped = clamp_cursor_base_size(base_size);
    let from_span = CURSOR_BASE_SIZE_MAX.saturating_sub(CURSOR_BASE_SIZE_MIN);
    let to_span = ACCESSIBILITY_CURSOR_SIZE_MAX.saturating_sub(ACCESSIBILITY_CURSOR_SIZE_MIN);
    if from_span == 0 || to_span == 0 {
        return ACCESSIBILITY_CURSOR_SIZE_MIN;
    }
    ACCESSIBILITY_CURSOR_SIZE_MIN
        + (clamped.saturating_sub(CURSOR_BASE_SIZE_MIN) * to_span + (from_span / 2)) / from_span
}

pub(crate) fn pointer_collection_summary() -> PointerCollectionSummary {
    summary::pointer_collection_summary()
}

pub(crate) fn scan_downloaded_collections() -> std::io::Result<PointerCollectionSummary> {
    summary::scan_downloaded_collections()
}

pub(crate) fn inspect_cursor_backup() -> std::io::Result<bool> {
    summary::ensure_directory_or_missing(&cache_root())?;
    match fs::symlink_metadata(backup_path()) {
        Ok(meta) if meta.file_type().is_file() && !summary::is_reparse(&meta) => Ok(true),
        Ok(_) => Err(std::io::Error::other("unsafe cursor backup")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

pub(crate) fn downloading_collection_count() -> usize {
    downloading_ids().lock().unwrap().len()
}

/// Downloads every missing pack on one worker that Clean All can cancel and
/// wait for, instead of one untracked thread per collection.
pub(crate) fn start_download_all_missing() -> usize {
    let root = cache_root();
    let missing = catalog_collections()
        .iter()
        .copied()
        .filter(|spec| !is_collection_complete(*spec, &root) && !is_downloading(spec.id))
        .collect::<Vec<_>>();
    if missing.is_empty() {
        return 0;
    }
    let stop = new_stop_signal();
    let Ok(activity) = crate::install_activity::register(stop.clone()) else {
        return 0;
    };
    let started = missing.len();
    thread::spawn(move || {
        let _activity = activity;
        let (tx, _rx) = mpsc::channel();
        for spec in missing {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            let _ = preload_collection(spec, &root, &tx, &stop);
        }
    });
    started
}

/// Rendered previews live beside the packs and are derived from them.
pub(super) const PREVIEW_CACHE_DIR: &str = ".preview-cache";

pub(crate) fn delete_all_downloaded_collections() -> Result<usize, String> {
    let deleted = delete_all_collections_in(&cache_root())?;
    if deleted > 0 {
        invalidate_summary_cache();
    }
    Ok(deleted)
}

fn delete_all_collections_in(root: &Path) -> Result<usize, String> {
    let mut deleted = 0usize;
    for spec in catalog_collections().iter().copied() {
        let mut removed = false;
        for dir in [spec.local_dir(root), spec.legacy_dir(root)] {
            if dir.exists() {
                fs::remove_dir_all(&dir)
                    .map_err(|error| format!("Failed to delete '{}': {error}", dir.display()))?;
                removed = true;
            }
        }
        deleted += usize::from(removed);
    }
    match fs::remove_dir_all(root.join(PREVIEW_CACHE_DIR)) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            return Err(format!("Failed to delete cursor previews: {error}"));
        }
        _ => {}
    }
    Ok(deleted)
}

pub(crate) fn delete_original_cursor_backup() -> Result<(), String> {
    let path = backup_path();
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.to_string()),
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("Original cursor backup path is unsafe".to_string());
    }
    fs::remove_file(path).map_err(|error| error.to_string())
}

pub(crate) fn has_original_cursor_backup() -> bool {
    backup_path().exists()
}

/// The scheme Windows reports as active, which survives app restarts and
/// identifies a pack this gallery applied earlier.
#[cfg(target_os = "windows")]
pub(super) fn active_cursor_scheme() -> Option<String> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey("Control Panel\\Cursors")
        .and_then(|key| key.get_value::<String, _>(""))
        .ok()
        .filter(|name| !name.is_empty())
}

#[cfg(not(target_os = "windows"))]
pub(super) fn active_cursor_scheme() -> Option<String> {
    None
}

#[cfg(target_os = "windows")]
pub(super) fn current_cursor_base_size() -> Result<u32, String> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (cursors_key, _) = hkcu
        .create_subkey("Control Panel\\Cursors")
        .map_err(|e| format!("Failed to open HKCU cursor key: {}", e))?;
    Ok(read_cursor_base_size_from_registry(&cursors_key))
}

#[cfg(not(target_os = "windows"))]
pub(super) fn current_cursor_base_size() -> Result<u32, String> {
    Err("Pointer size changes are only supported on Windows.".to_string())
}

#[cfg(target_os = "windows")]
pub(super) fn apply_cursor_base_size(size: u32) -> Result<u32, String> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    backup_current_cursor_settings_if_needed()?;

    let applied_size = clamp_cursor_base_size(size);
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (cursors_key, _) = hkcu
        .create_subkey("Control Panel\\Cursors")
        .map_err(|e| format!("Failed to open HKCU cursor key: {}", e))?;
    cursors_key
        .set_value("CursorBaseSize", &applied_size)
        .map_err(|e| format!("Failed writing cursor size: {}", e))?;
    write_accessibility_cursor_size(&hkcu, accessibility_cursor_size_from_base(applied_size))?;
    refresh_cursor_settings();

    Ok(applied_size)
}

#[cfg(not(target_os = "windows"))]
pub(super) fn apply_cursor_base_size(_size: u32) -> Result<u32, String> {
    Err("Pointer size changes are only supported on Windows.".to_string())
}

#[cfg(target_os = "windows")]
pub(super) fn restore_original_cursor_backup() -> Result<(), String> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    let path = backup_path();
    let bytes = fs::read(&path).map_err(|e| format!("Failed reading backup file: {}", e))?;
    let backup: CursorBackup =
        serde_json::from_slice(&bytes).map_err(|e| format!("Failed parsing backup file: {}", e))?;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (cursors_key, _) = hkcu
        .create_subkey("Control Panel\\Cursors")
        .map_err(|e| format!("Failed to open HKCU cursor key: {}", e))?;

    for (name, value) in backup.values {
        cursors_key
            .set_value(name.as_str(), &value)
            .map_err(|e| format!("Failed restoring '{}': {}", name, e))?;
    }
    if let Some(size) = backup.cursor_base_size {
        let clamped = clamp_cursor_base_size(size);
        cursors_key
            .set_value("CursorBaseSize", &clamped)
            .map_err(|e| format!("Failed restoring cursor size: {}", e))?;
        write_accessibility_cursor_size(&hkcu, accessibility_cursor_size_from_base(clamped))?;
    }
    refresh_cursor_settings();
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub(super) fn restore_original_cursor_backup() -> Result<(), String> {
    Err("Cursor restore is only supported on Windows.".to_string())
}

pub(super) fn preload_collection(
    spec: CursorCollectionSpec,
    cache_root: &Path,
    tx: &Sender<GalleryEvent>,
    stop_signal: &AtomicBool,
) -> Result<HashMap<String, PathBuf>, String> {
    // A worker that is still winding down owns the folder until it notices
    // its stop signal; two writers would delete each other's temp files.
    let _claim = loop {
        if let Some(claim) = claim_download(spec.id) {
            break claim;
        }
        if stop_signal.load(Ordering::Relaxed) {
            return Ok(HashMap::new());
        }
        thread::sleep(std::time::Duration::from_millis(50));
    };
    preload_collection_inner(spec, cache_root, tx, stop_signal)
}

fn preload_collection_inner(
    spec: CursorCollectionSpec,
    cache_root: &Path,
    tx: &Sender<GalleryEvent>,
    stop_signal: &AtomicBool,
) -> Result<HashMap<String, PathBuf>, String> {
    let local_dir = spec.local_dir(cache_root);
    fs::create_dir_all(&local_dir)
        .map_err(|e| format!("Failed to create local folder for {}: {}", spec.title, e))?;

    let mut files = HashMap::new();
    for file_name in REQUIRED_FILES {
        if stop_signal.load(Ordering::Relaxed) {
            return Ok(files);
        }
        let local_path = local_dir.join(file_name);
        if local_path.exists() {
            files.insert(file_name.to_string(), local_path.clone());
            let _ = tx.send(GalleryEvent::FileReady {
                id: spec.id.to_string(),
                file_name: file_name.to_string(),
                path: local_path,
            });
        }
    }

    if files.len() == REQUIRED_FILES.len() {
        let _ = tx.send(GalleryEvent::Progress {
            id: spec.id.to_string(),
            downloaded: REQUIRED_FILES.len(),
            total: REQUIRED_FILES.len(),
        });
        return Ok(files);
    }

    let total = REQUIRED_FILES.len();
    let mut downloaded = files.len();
    let _ = tx.send(GalleryEvent::Progress {
        id: spec.id.to_string(),
        downloaded,
        total,
    });

    match spec.source {
        CollectionSource::Github(source) => preload_missing_from_github(
            spec,
            &local_dir,
            tx,
            stop_signal,
            &mut files,
            &mut downloaded,
            total,
            source,
        )?,
        CollectionSource::ZipArchive { url, subdir, pin } => preload_missing_from_zip(
            spec,
            &local_dir,
            tx,
            stop_signal,
            &mut files,
            &mut downloaded,
            total,
            url,
            subdir,
            pin,
        )?,
        CollectionSource::RarArchive { url, subdir, pin } => preload_missing_from_rar(
            spec,
            &local_dir,
            tx,
            stop_signal,
            &mut files,
            &mut downloaded,
            total,
            url,
            subdir,
            pin,
        )?,
    }

    Ok(files)
}

pub(super) fn expected_file_count() -> usize {
    REQUIRED_FILES.len()
}

pub(super) fn is_complete_files(files: &HashMap<String, PathBuf>) -> bool {
    files.len() == REQUIRED_FILES.len()
}

pub(super) fn required_file_names() -> &'static [&'static str] {
    &REQUIRED_FILES
}

pub(super) fn collection_specs() -> &'static [CursorCollectionSpec] {
    catalog_collections()
}

pub(super) fn is_collection_downloading(id: &str) -> bool {
    downloading_ids().lock().unwrap().contains(id)
}

#[cfg(target_os = "windows")]
fn read_cursor_base_size_from_registry(cursors_key: &winreg::RegKey) -> u32 {
    let dword = cursors_key.get_value::<u32, _>("CursorBaseSize").ok();
    let parsed_string = cursors_key
        .get_value::<String, _>("CursorBaseSize")
        .ok()
        .and_then(|raw| raw.trim().parse::<u32>().ok());
    let raw = dword.or(parsed_string).unwrap_or(CURSOR_BASE_SIZE_DEFAULT);
    clamp_cursor_base_size(raw)
}

#[cfg(target_os = "windows")]
fn backup_current_cursor_settings_if_needed() -> Result<(), String> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    let backup = backup_path();
    if backup.exists() {
        return Ok(());
    }

    if let Some(parent) = backup.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed creating backup folder: {}", e))?;
    }

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (cursors_key, _) = hkcu
        .create_subkey("Control Panel\\Cursors")
        .map_err(|e| format!("Failed to open HKCU cursor key: {}", e))?;

    let mut values = HashMap::new();
    values.insert(
        "".to_string(),
        cursors_key.get_value::<String, _>("").unwrap_or_default(),
    );
    for (name, _) in REGISTRY_FILE_MAP {
        values
            .entry(name.to_string())
            .or_insert_with(|| cursors_key.get_value::<String, _>(name).unwrap_or_default());
    }

    let payload = CursorBackup {
        values,
        cursor_base_size: Some(read_cursor_base_size_from_registry(&cursors_key)),
    };
    cursor_backup::write(&backup, &payload)
}

#[cfg(not(target_os = "windows"))]
fn backup_current_cursor_settings_if_needed() -> Result<(), String> {
    Ok(())
}

#[cfg(target_os = "windows")]
pub(super) fn apply_downloaded_collection(
    spec: CursorCollectionSpec,
    files: &HashMap<String, PathBuf>,
    desired_cursor_base_size: u32,
    live_preview_only: bool,
) -> Result<(), String> {
    use windows::Win32::UI::WindowsAndMessaging::OCR_NORMAL;
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    backup_current_cursor_settings_if_needed()?;

    for file_name in REQUIRED_FILES {
        if !files.contains_key(file_name) {
            return Err(format!("Cannot apply pack, missing file '{}'", file_name));
        }
    }

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (cursors_key, _) = hkcu
        .create_subkey("Control Panel\\Cursors")
        .map_err(|e| format!("Failed to open HKCU cursor key: {}", e))?;
    let clamped_size = clamp_cursor_base_size(desired_cursor_base_size);
    let effective_files = scaled_cursor_file_map_for_size(files, clamped_size, live_preview_only);

    for (value_name, path) in
        registry_values::cursor_values(files, &effective_files, live_preview_only)?
    {
        cursors_key
            .set_value(value_name, &path)
            .map_err(|e| format!("Failed writing '{}': {}", value_name, e))?;
    }

    cursors_key
        .set_value("CursorBaseSize", &CURSOR_BASE_SIZE_DEFAULT)
        .map_err(|e| format!("Failed writing cursor size: {}", e))?;
    write_accessibility_cursor_size(&hkcu, ACCESSIBILITY_CURSOR_SIZE_MIN)?;

    if live_preview_only {
        if let Some(arrow_path) = effective_files
            .get("pointer.cur")
            .or_else(|| files.get("pointer.cur"))
        {
            let _ = system::set_system_cursor_from_file(arrow_path, OCR_NORMAL, Some(clamped_size));
        }
        return Ok(());
    }

    cursors_key
        .set_value("", &spec.scheme_name)
        .map_err(|e| format!("Failed writing active scheme name: {}", e))?;
    let _ = cursors_key.set_value("Scheme Source", &1u32);

    let (schemes_key, _) = hkcu
        .create_subkey("Control Panel\\Cursors\\Schemes")
        .map_err(|e| format!("Failed to open HKCU scheme key: {}", e))?;
    let scheme_value = registry_values::scheme_list(&effective_files)?;
    schemes_key
        .set_value(spec.scheme_name, &scheme_value)
        .map_err(|e| format!("Failed writing scheme list: {}", e))?;

    refresh_cursor_settings();
    system::apply_standard_system_cursors(&effective_files, files, clamped_size);

    Ok(())
}

pub(super) fn new_stop_signal() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}

/// Stops whichever worker owns `current` and gives the next worker its own
/// flag, so resuming can never revive a predecessor that is still winding down.
pub(super) fn replace_stop_signal(current: &mut Arc<AtomicBool>) -> Arc<AtomicBool> {
    current.store(true, Ordering::Relaxed);
    *current = new_stop_signal();
    current.clone()
}

#[cfg(not(target_os = "windows"))]
pub(super) fn apply_downloaded_collection(
    _spec: CursorCollectionSpec,
    _files: &HashMap<String, PathBuf>,
    _desired_cursor_base_size: u32,
    _live_preview_only: bool,
) -> Result<(), String> {
    Err("Applying cursor packs is only supported on Windows.".to_string())
}

#[cfg(test)]
mod download_gate_tests;
