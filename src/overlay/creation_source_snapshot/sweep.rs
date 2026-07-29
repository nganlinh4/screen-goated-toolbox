use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{
    ORPHAN_GRACE_MS, SNAPSHOT_LOCK, cleanup_manifest_directory, cleanup_uncommitted_directory,
    is_reparse_point, now_ms, presentation::PRESENTATION_LOCK, preview_root, read_manifest,
    snapshot_id_from_descriptors, snapshot_root, valid_id, validate_manifest, write_manifest,
};

const MAX_SCANNED_ENTRIES: usize = 4_096;
const MAX_DELETIONS_PER_PASS: usize = 256;
const CURSOR_NAME: &str = ".sgt-source-sweep.json";
const CURSOR_TEMP_NAME: &str = ".sgt-source-sweep.json.tmp";
const MAX_CURSOR_BYTES: u64 = 512;

#[derive(Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SweepCursor {
    last_name: String,
}

pub(crate) fn sweep() -> Result<(), String> {
    sweep_with_grace(ORPHAN_GRACE_MS)
}

pub(crate) fn sweep_pressure() -> Result<(), String> {
    sweep_with_grace(0)
}

fn sweep_with_grace(grace_ms: u64) -> Result<(), String> {
    let _presentation_guard = PRESENTATION_LOCK
        .lock()
        .map_err(|_| "Creation preview cleanup is unavailable.".to_string())?;
    let protected_dispatches = crate::overlay::creation_delivery::protected_dispatch_ids()?;
    let pending_previews = crate::overlay::creation_delivery::pending_source_paths()?;
    let live_previews = crate::overlay::generation_history::live_source_paths()?;
    let intents = crate::overlay::creation_intent_journal::load_all()?;
    let active_dispatches = intents
        .iter()
        .map(|intent| intent.dispatch_id.clone())
        .chain(protected_dispatches.iter().cloned())
        .collect::<HashSet<_>>();
    let root = snapshot_root()?;
    let mut protected_snapshots = HashSet::new();
    for intent in &intents {
        let descriptors = intent
            .arguments
            .get("sourceDescriptors")
            .and_then(|value| {
                serde_json::from_value::<Vec<crate::overlay::creation_source::SourceDescriptor>>(
                    value.clone(),
                )
                .ok()
            })
            .unwrap_or_default();
        if !descriptors.is_empty()
            && let Ok(snapshot_id) = snapshot_id_from_descriptors(&root, &descriptors)
        {
            protected_snapshots.insert(snapshot_id);
        }
    }
    let mut protected_previews = pending_previews
        .into_iter()
        .chain(live_previews)
        .map(|path| path.to_ascii_lowercase())
        .collect();
    let _guard = SNAPSHOT_LOCK
        .lock()
        .map_err(|_| "Creation source cleanup is unavailable.".to_string())?;
    sweep_snapshots(
        &root,
        &active_dispatches,
        &protected_snapshots,
        &mut protected_previews,
        now_ms(),
        grace_ms,
    )?;
    sweep_previews(&preview_root()?, &protected_previews, now_ms(), grace_ms)
}

fn sweep_snapshots(
    root: &Path,
    active_dispatches: &HashSet<String>,
    protected_snapshots: &HashSet<String>,
    protected_previews: &mut HashSet<String>,
    now: u64,
    grace_ms: u64,
) -> Result<(), String> {
    let mut deleted = 0_usize;
    let cursor = read_cursor(root).unwrap_or_default();
    let (page, page_has_more) = ordered_page(root, &cursor.last_name)?;
    let mut last_name = String::new();
    let mut stopped_early = false;
    for (name, path) in page {
        if deleted >= MAX_DELETIONS_PER_PASS {
            stopped_early = true;
            break;
        }
        last_name = name.clone();
        let metadata = match std::fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return Err("Creation source cleanup is unavailable.".to_string()),
        };
        if name == CURSOR_TEMP_NAME {
            if metadata.file_type().is_file()
                && !is_reparse_point(&metadata)
                && modified_ms(&metadata)
                    .is_some_and(|modified| now.saturating_sub(modified) >= grace_ms)
                && std::fs::remove_file(path).is_ok()
            {
                deleted += 1;
            }
            continue;
        }
        if !valid_id(&name)
            || !metadata.file_type().is_dir()
            || is_reparse_point(&metadata)
            || protected_snapshots.contains(&name)
        {
            continue;
        }
        let old_enough =
            modified_ms(&metadata).is_some_and(|modified| now.saturating_sub(modified) >= grace_ms);
        let Ok(mut manifest) = read_manifest(&path) else {
            if old_enough && cleanup_uncommitted_directory(&path).is_ok() {
                deleted += 1;
            }
            continue;
        };
        if validate_manifest(root, &manifest).is_err() {
            if old_enough && cleanup_uncommitted_directory(&path).is_ok() {
                deleted += 1;
            }
            continue;
        }
        let previous_owners = manifest.intent_owners.len();
        manifest
            .intent_owners
            .retain(|owner| active_dispatches.contains(owner));
        if manifest
            .continuation
            .as_ref()
            .is_some_and(|owner| owner.expires_at_ms <= now)
        {
            manifest.continuation = None;
        }
        if manifest.intent_owners.is_empty() && manifest.continuation.is_none() && old_enough {
            if cleanup_manifest_directory(&path, &manifest).is_ok() {
                deleted += 1;
            } else {
                protect_manifest_previews(&manifest, protected_previews);
            }
        } else {
            protect_manifest_previews(&manifest, protected_previews);
            if previous_owners != manifest.intent_owners.len() {
                write_manifest(&path, &manifest)?;
            }
        }
    }
    finish_cursor(root, page_has_more || stopped_early, last_name)?;
    Ok(())
}

fn sweep_previews(
    root: &Path,
    protected_paths: &HashSet<String>,
    now: u64,
    grace_ms: u64,
) -> Result<(), String> {
    let mut deleted = 0_usize;
    let cursor = read_cursor(root).unwrap_or_default();
    let (page, page_has_more) = ordered_page(root, &cursor.last_name)?;
    let mut last_name = String::new();
    let mut stopped_early = false;
    for (name, path) in page {
        if deleted >= MAX_DELETIONS_PER_PASS {
            stopped_early = true;
            break;
        }
        last_name = name.clone();
        let metadata = match std::fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return Err("Creation preview cleanup is unavailable.".to_string()),
        };
        let final_name = name.len() == 68
            && name.ends_with(".png")
            && name[..64].bytes().all(|byte| byte.is_ascii_hexdigit());
        let temporary = (name.starts_with(".sgt-preview-") && name.ends_with(".tmp"))
            || name == CURSOR_TEMP_NAME;
        let valid_name = final_name || temporary;
        if !valid_name
            || !metadata.file_type().is_file()
            || is_reparse_point(&metadata)
            || (!temporary
                && protected_paths.contains(&path.to_string_lossy().to_ascii_lowercase()))
            || modified_ms(&metadata).is_none_or(|modified| now.saturating_sub(modified) < grace_ms)
        {
            continue;
        }
        if std::fs::remove_file(path).is_ok() {
            deleted += 1;
        }
    }
    finish_cursor(root, page_has_more || stopped_early, last_name)?;
    Ok(())
}

fn protect_manifest_previews(manifest: &super::SnapshotManifest, protected: &mut HashSet<String>) {
    protected.extend(
        manifest
            .presentation_paths
            .iter()
            .map(|path| path.to_ascii_lowercase()),
    );
}

fn ordered_page(root: &Path, after: &str) -> Result<(Vec<(String, PathBuf)>, bool), String> {
    let mut page = BTreeMap::new();
    let mut has_more = false;
    for entry in
        std::fs::read_dir(root).map_err(|_| "Creation cleanup is unavailable.".to_string())?
    {
        let entry = entry.map_err(|_| "Creation cleanup is unavailable.".to_string())?;
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if name == CURSOR_NAME || name.as_str() <= after {
            continue;
        }
        page.insert(name, entry.path());
        if page.len() > MAX_SCANNED_ENTRIES {
            page.pop_last();
            has_more = true;
        }
    }
    Ok((page.into_iter().collect(), has_more))
}

fn read_cursor(root: &Path) -> Result<SweepCursor, String> {
    use std::io::Read as _;
    let path = root.join(CURSOR_NAME);
    let metadata = match std::fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_file() && !is_reparse_point(&metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(SweepCursor::default());
        }
        _ => return Err("Creation cleanup cursor is invalid.".to_string()),
    };
    if metadata.len() > MAX_CURSOR_BYTES {
        return Err("Creation cleanup cursor is invalid.".to_string());
    }
    let file = std::fs::File::open(path)
        .map_err(|_| "Creation cleanup cursor is unavailable.".to_string())?;
    serde_json::from_reader(file.take(MAX_CURSOR_BYTES + 1))
        .map_err(|_| "Creation cleanup cursor is invalid.".to_string())
}

fn finish_cursor(root: &Path, has_more: bool, last_name: String) -> Result<(), String> {
    let path = root.join(CURSOR_NAME);
    if !has_more {
        return match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err("Creation cleanup cursor could not be cleared.".to_string()),
        };
    }
    if last_name.is_empty() {
        return Err("Creation cleanup cursor is invalid.".to_string());
    }
    match std::fs::remove_file(&path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err("Creation cleanup cursor could not be replaced.".to_string()),
    }
    crate::atomic_json::write_json_atomic(&path, &SweepCursor { last_name })
        .map_err(|_| "Creation cleanup cursor could not be saved.".to_string())
}

fn modified_ms(metadata: &std::fs::Metadata) -> Option<u64> {
    metadata
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_millis()
        .try_into()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_sweep_converges_across_multiple_deterministic_pages() {
        let root = std::env::temp_dir().join(format!(
            "sgt-preview-sweep-{}-{}",
            std::process::id(),
            crate::overlay::creation_identity::random_id("case-").unwrap()
        ));
        std::fs::create_dir(&root).unwrap();
        let protected = root.join(format!("{:064x}.png", 300_u64));
        for index in 0..600_u64 {
            std::fs::write(root.join(format!("{index:064x}.png")), b"preview").unwrap();
        }
        let protected_paths = HashSet::from([protected.to_string_lossy().to_ascii_lowercase()]);

        for _ in 0..6 {
            sweep_previews(&root, &protected_paths, u64::MAX, ORPHAN_GRACE_MS).unwrap();
        }

        let remaining = std::fs::read_dir(&root)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>();
        assert_eq!(remaining, vec![protected]);
        std::fs::remove_dir_all(root).unwrap();
    }
}
