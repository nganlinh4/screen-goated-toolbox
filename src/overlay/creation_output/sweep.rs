use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use serde::{Deserialize, Serialize};

use super::{
    MAX_SCANNED_STAGING_ENTRIES, STAGING_ORPHAN_GRACE_MS, is_reparse_point, now_ms,
    read_staging_marker, remove_empty_or_marker_only_directory, remove_owned_staging_directory,
    staging_root, valid_dispatch_id,
};

const CURSOR_NAME: &str = ".sgt-staging-sweep.json";
const CURSOR_TEMP_NAME: &str = ".sgt-staging-sweep.json.tmp";
const MAX_CURSOR_BYTES: u64 = 512;
static SWEEP_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

#[derive(Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SweepCursor {
    last_name: String,
}

pub(crate) fn sweep_staging() -> Result<(), String> {
    let protected = crate::overlay::creation_intent_journal::load_all()?
        .into_iter()
        .map(|intent| intent.dispatch_id)
        .chain(crate::overlay::creation_delivery::protected_dispatch_ids()?)
        .collect();
    sweep_staging_at(
        &staging_root()?,
        &protected,
        now_ms(),
        STAGING_ORPHAN_GRACE_MS,
    )
}

pub(super) fn sweep_staging_at(
    root: &Path,
    protected: &HashSet<String>,
    now: u64,
    grace_ms: u64,
) -> Result<(), String> {
    sweep_staging_bounded_at(root, protected, now, grace_ms, MAX_SCANNED_STAGING_ENTRIES)
}

pub(super) fn sweep_staging_bounded_at(
    root: &Path,
    protected: &HashSet<String>,
    now: u64,
    grace_ms: u64,
    max_entries: usize,
) -> Result<(), String> {
    let _guard = SWEEP_LOCK
        .lock()
        .map_err(|_| "Creation staging cleanup is unavailable.".to_string())?;
    let root = std::fs::canonicalize(root)
        .map_err(|_| "Creation staging cleanup is unavailable.".to_string())?;
    let cursor = read_cursor(&root).unwrap_or_default();
    let (page, has_more) = ordered_page(&root, &cursor.last_name, max_entries)?;
    for (_, path) in &page {
        sweep_entry(path, protected, now, grace_ms)?;
    }
    if has_more {
        let last_name = page
            .last()
            .map(|(name, _)| name.clone())
            .ok_or_else(|| "Creation staging cleanup cursor is invalid.".to_string())?;
        write_cursor(&root, &SweepCursor { last_name })?;
    } else {
        clear_cursor(&root)?;
    }
    Ok(())
}

fn ordered_page(
    root: &Path,
    after: &str,
    max_entries: usize,
) -> Result<(Vec<(String, PathBuf)>, bool), String> {
    if max_entries == 0 {
        return Err("Creation staging cleanup page is invalid.".to_string());
    }
    let mut page = BTreeMap::new();
    let mut has_more = false;
    for entry in std::fs::read_dir(root)
        .map_err(|_| "Creation staging cleanup is unavailable.".to_string())?
    {
        let entry = entry.map_err(|_| "Creation staging cleanup is unavailable.".to_string())?;
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if name == CURSOR_NAME || name.as_str() <= after {
            continue;
        }
        page.insert(name, entry.path());
        if page.len() > max_entries {
            page.pop_last();
            has_more = true;
        }
    }
    Ok((page.into_iter().collect(), has_more))
}

fn sweep_entry(
    path: &Path,
    protected: &HashSet<String>,
    now: u64,
    grace_ms: u64,
) -> Result<(), String> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err("Creation staging cleanup is unavailable.".to_string()),
    };
    if is_reparse_point(&metadata) {
        return Ok(());
    }
    let Some(dispatch_id) = path.file_name().and_then(|name| name.to_str()) else {
        return Ok(());
    };
    if dispatch_id == CURSOR_TEMP_NAME {
        if directory_age_ms(&metadata, now) >= grace_ms && metadata.file_type().is_file() {
            let _ = std::fs::remove_file(path);
        }
        return Ok(());
    }
    if !metadata.file_type().is_dir()
        || !valid_dispatch_id(dispatch_id)
        || protected.contains(dispatch_id)
    {
        return Ok(());
    }
    let age = directory_age_ms(&metadata, now);
    let Ok(marker) = read_staging_marker(path) else {
        if age >= grace_ms {
            let _ = remove_empty_or_marker_only_directory(path);
        }
        return Ok(());
    };
    if marker.dispatch_id == dispatch_id
        && marker.created_at_ms <= now
        && now.saturating_sub(marker.created_at_ms) >= grace_ms
    {
        let _ = remove_owned_staging_directory(path, &marker);
    }
    Ok(())
}

fn read_cursor(root: &Path) -> Result<SweepCursor, String> {
    use std::io::Read as _;
    let path = root.join(CURSOR_NAME);
    let metadata = match std::fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_file() && !is_reparse_point(&metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(SweepCursor::default());
        }
        _ => return Err("Creation staging cleanup cursor is invalid.".to_string()),
    };
    if metadata.len() > MAX_CURSOR_BYTES {
        return Err("Creation staging cleanup cursor is invalid.".to_string());
    }
    let file = std::fs::File::open(path)
        .map_err(|_| "Creation staging cleanup cursor is unavailable.".to_string())?;
    serde_json::from_reader(file.take(MAX_CURSOR_BYTES + 1))
        .map_err(|_| "Creation staging cleanup cursor is invalid.".to_string())
}

fn write_cursor(root: &Path, cursor: &SweepCursor) -> Result<(), String> {
    clear_cursor(root)?;
    crate::atomic_json::write_json_atomic(&root.join(CURSOR_NAME), cursor)
        .map_err(|_| "Creation staging cleanup cursor could not be saved.".to_string())
}

fn clear_cursor(root: &Path) -> Result<(), String> {
    match std::fs::remove_file(root.join(CURSOR_NAME)) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("Creation staging cleanup cursor could not be cleared.".to_string()),
    }
}

fn directory_age_ms(metadata: &std::fs::Metadata, now: u64) -> u64 {
    metadata
        .created()
        .or_else(|_| metadata.modified())
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .map(|created| now.saturating_sub(created))
        .unwrap_or(0)
}
