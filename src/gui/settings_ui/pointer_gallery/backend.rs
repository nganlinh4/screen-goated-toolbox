use super::GalleryEvent;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};
mod catalog;
mod sources;
use catalog::collections as catalog_collections;
pub(super) use catalog::CursorCollectionSpec;
use catalog::{CollectionSource, REGISTRY_FILE_MAP, REQUIRED_FILES, SCHEME_FILE_ORDER};
use sources::{preload_missing_from_github, preload_missing_from_rar, preload_missing_from_zip};

#[derive(Clone, Copy)]
pub(crate) struct PointerCollectionSummary {
    pub downloaded_count: usize,
    pub total_count: usize,
    pub downloading_count: usize,
    pub downloaded_bytes: u64,
}

#[derive(Serialize, Deserialize)]
struct CursorBackup {
    values: HashMap<String, String>,
}

#[derive(Clone, Copy)]
struct PointerSummaryDiskSnapshot {
    downloaded_count: usize,
    total_count: usize,
    downloaded_bytes: u64,
}

struct PointerSummaryCache {
    last_scan: Option<Instant>,
    snapshot: PointerSummaryDiskSnapshot,
}

const SUMMARY_CACHE_TTL: Duration = Duration::from_millis(1500);

fn cache_root() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("screen-goated-toolbox")
        .join("pointer-gallery")
}

fn backup_path() -> PathBuf {
    cache_root().join("original-cursor-backup.json")
}

fn downloading_ids() -> &'static Mutex<HashSet<String>> {
    static IDS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    IDS.get_or_init(|| Mutex::new(HashSet::new()))
}

fn summary_cache() -> &'static Mutex<PointerSummaryCache> {
    static CACHE: OnceLock<Mutex<PointerSummaryCache>> = OnceLock::new();
    CACHE.get_or_init(|| {
        Mutex::new(PointerSummaryCache {
            last_scan: None,
            snapshot: PointerSummaryDiskSnapshot {
                downloaded_count: 0,
                total_count: catalog_collections().len(),
                downloaded_bytes: 0,
            },
        })
    })
}

fn invalidate_summary_cache() {
    if let Ok(mut cache) = summary_cache().lock() {
        cache.last_scan = None;
    }
}

fn set_downloading(id: &str, downloading: bool) {
    {
        let mut ids = downloading_ids().lock().unwrap();
        if downloading {
            ids.insert(id.to_string());
        } else {
            ids.remove(id);
        }
    }
    invalidate_summary_cache();
}

fn start_downloading_if_idle(id: &str) -> bool {
    let mut ids = downloading_ids().lock().unwrap();
    if ids.contains(id) {
        return false;
    }
    ids.insert(id.to_string());
    true
}

fn is_collection_complete(spec: CursorCollectionSpec, cache_root: &Path) -> bool {
    let dir = spec.local_dir(cache_root);
    REQUIRED_FILES.iter().all(|name| dir.join(name).exists())
}

fn collection_downloaded_size(spec: CursorCollectionSpec, cache_root: &Path) -> u64 {
    dir_size_bytes(&spec.local_dir(cache_root))
}

fn dir_size_bytes(path: &Path) -> u64 {
    let Ok(entries) = fs::read_dir(path) else {
        return 0;
    };

    let mut total = 0u64;
    for entry in entries.flatten() {
        if let Ok(meta) = entry.metadata() {
            if meta.is_dir() {
                total += dir_size_bytes(&entry.path());
            } else {
                total += meta.len();
            }
        }
    }

    total
}

fn compute_pointer_summary_disk(root: &Path) -> PointerSummaryDiskSnapshot {
    let mut downloaded_count = 0usize;
    let mut downloaded_bytes = 0u64;
    for spec in catalog_collections().iter().copied() {
        downloaded_bytes += collection_downloaded_size(spec, root);
        if is_collection_complete(spec, root) {
            downloaded_count += 1;
        }
    }

    PointerSummaryDiskSnapshot {
        downloaded_count,
        total_count: catalog_collections().len(),
        downloaded_bytes,
    }
}

pub(crate) fn pointer_collection_summary() -> PointerCollectionSummary {
    let downloading_count = downloading_ids().lock().unwrap().len();
    let now = Instant::now();
    let root = cache_root();
    let snapshot = {
        let mut cache = summary_cache().lock().unwrap();
        let needs_refresh = cache
            .last_scan
            .is_none_or(|last| now.duration_since(last) >= SUMMARY_CACHE_TTL);
        if needs_refresh {
            cache.snapshot = compute_pointer_summary_disk(&root);
            cache.last_scan = Some(now);
        }
        cache.snapshot
    };

    PointerCollectionSummary {
        downloaded_count: snapshot.downloaded_count,
        total_count: snapshot.total_count,
        downloading_count,
        downloaded_bytes: snapshot.downloaded_bytes,
    }
}

pub(crate) fn start_download_all_missing() -> usize {
    let root = cache_root();
    let mut started = 0usize;

    for spec in catalog_collections().iter().copied() {
        if is_collection_complete(spec, &root) {
            continue;
        }

        if !start_downloading_if_idle(spec.id) {
            continue;
        }

        let root_for_thread = root.clone();
        thread::spawn(move || {
            let (tx, _rx) = mpsc::channel();
            let stop = AtomicBool::new(false);
            let _ = preload_collection(spec, &root_for_thread, &tx, &stop);
        });

        started += 1;
    }

    started
}

pub(crate) fn delete_all_downloaded_collections() -> usize {
    let root = cache_root();
    let mut deleted = 0usize;

    for spec in catalog_collections().iter().copied() {
        let dir = spec.local_dir(&root);
        if dir.exists() && fs::remove_dir_all(&dir).is_ok() {
            deleted += 1;
        }
    }

    if deleted > 0 {
        invalidate_summary_cache();
    }

    deleted
}

pub(crate) fn has_original_cursor_backup() -> bool {
    backup_path().exists()
}

#[cfg(target_os = "windows")]
pub(crate) fn restore_original_cursor_backup() -> Result<(), String> {
    use windows::Win32::UI::WindowsAndMessaging::{
        SystemParametersInfoW, SPIF_SENDCHANGE, SPIF_UPDATEINIFILE, SPI_SETCURSORS,
        SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    };
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

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

    unsafe {
        let flags = SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(SPIF_SENDCHANGE.0 | SPIF_UPDATEINIFILE.0);
        let _ = SystemParametersInfoW(SPI_SETCURSORS, 0, None, flags);
    }

    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn restore_original_cursor_backup() -> Result<(), String> {
    Err("Cursor restore is only supported on Windows.".to_string())
}

pub(super) fn preload_collection(
    spec: CursorCollectionSpec,
    cache_root: &Path,
    tx: &Sender<GalleryEvent>,
    stop_signal: &AtomicBool,
) -> Result<HashMap<String, PathBuf>, String> {
    set_downloading(spec.id, true);
    let result = preload_collection_inner(spec, cache_root, tx, stop_signal);
    set_downloading(spec.id, false);
    invalidate_summary_cache();
    result
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
        CollectionSource::GithubApi(api_urls) => preload_missing_from_github(
            spec,
            &local_dir,
            tx,
            stop_signal,
            &mut files,
            &mut downloaded,
            total,
            api_urls,
        )?,
        CollectionSource::ZipArchive { url, subdir } => preload_missing_from_zip(
            spec,
            &local_dir,
            tx,
            stop_signal,
            &mut files,
            &mut downloaded,
            total,
            url,
            subdir,
        )?,
        CollectionSource::RarArchive { url, subdir } => preload_missing_from_rar(
            spec,
            &local_dir,
            tx,
            stop_signal,
            &mut files,
            &mut downloaded,
            total,
            url,
            subdir,
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
fn backup_current_cursor_settings_if_needed() -> Result<(), String> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

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

    let payload = CursorBackup { values };
    let bytes = serde_json::to_vec_pretty(&payload)
        .map_err(|e| format!("Failed encoding backup: {}", e))?;
    fs::write(&backup, bytes).map_err(|e| format!("Failed writing backup file: {}", e))?;
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn backup_current_cursor_settings_if_needed() -> Result<(), String> {
    Ok(())
}

#[cfg(target_os = "windows")]
pub(super) fn apply_downloaded_collection(
    spec: CursorCollectionSpec,
    files: &HashMap<String, PathBuf>,
) -> Result<(), String> {
    use windows::Win32::UI::WindowsAndMessaging::{
        SystemParametersInfoW, SPIF_SENDCHANGE, SPIF_UPDATEINIFILE, SPI_SETCURSORS,
        SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    };
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

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

    for (value_name, file_name) in REGISTRY_FILE_MAP {
        let Some(path) = files.get(file_name) else {
            return Err(format!("Cannot apply pack, missing file '{}'", file_name));
        };
        cursors_key
            .set_value(value_name, &to_registry_path(path))
            .map_err(|e| format!("Failed writing '{}': {}", value_name, e))?;
    }

    cursors_key
        .set_value("", &spec.scheme_name)
        .map_err(|e| format!("Failed writing active scheme name: {}", e))?;
    let _ = cursors_key.set_value("Scheme Source", &1u32);

    let (schemes_key, _) = hkcu
        .create_subkey("Control Panel\\Cursors\\Schemes")
        .map_err(|e| format!("Failed to open HKCU scheme key: {}", e))?;
    let scheme_value = SCHEME_FILE_ORDER
        .iter()
        .map(|name| {
            files
                .get(*name)
                .map(|path| to_registry_path(path))
                .ok_or_else(|| format!("Missing file '{}'", name))
        })
        .collect::<Result<Vec<_>, _>>()?
        .join(",");
    schemes_key
        .set_value(spec.scheme_name, &scheme_value)
        .map_err(|e| format!("Failed writing scheme list: {}", e))?;

    unsafe {
        let flags = SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(SPIF_SENDCHANGE.0 | SPIF_UPDATEINIFILE.0);
        let _ = SystemParametersInfoW(SPI_SETCURSORS, 0, None, flags);
    }

    Ok(())
}

pub(super) fn new_stop_signal() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}

#[cfg(not(target_os = "windows"))]
pub(super) fn apply_downloaded_collection(
    _spec: CursorCollectionSpec,
    _files: &HashMap<String, PathBuf>,
) -> Result<(), String> {
    Err("Applying cursor packs is only supported on Windows.".to_string())
}

fn to_registry_path(path: &Path) -> String {
    path.to_string_lossy().replace('/', "\\")
}
