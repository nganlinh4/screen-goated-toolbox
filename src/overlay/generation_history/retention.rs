use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use sha2::{Digest as _, Sha256};

use super::*;

pub(super) fn digest_file(path: &Path) -> Result<(u64, String), String> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| format!("Could not inspect artifact: {error}"))?;
    if !metadata.file_type().is_file() {
        return Err("Artifact is not a regular file.".to_string());
    }
    let size = metadata.len();
    if size > MAX_HISTORY_ARTIFACT_BYTES {
        return Err("Artifact exceeds the managed history size limit.".to_string());
    }
    let mut file = std::fs::File::open(path)
        .map_err(|error| format!("Could not inspect artifact: {error}"))?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut read_total = 0_u64;
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("Could not inspect artifact: {error}"))?;
        if read == 0 {
            break;
        }
        read_total = read_total.saturating_add(read as u64);
        if read_total > size || read_total > MAX_HISTORY_ARTIFACT_BYTES {
            return Err("Artifact changed while it was being inspected.".to_string());
        }
        digest.update(&buffer[..read]);
    }
    if read_total != size {
        return Err("Artifact changed while it was being inspected.".to_string());
    }
    Ok((size, format!("{:x}", digest.finalize())))
}

fn is_managed_artifact_under(path: &Path, managed_root: &Path) -> bool {
    let (Ok(path), Ok(root)) = (
        std::fs::canonicalize(path),
        std::fs::canonicalize(managed_root),
    ) else {
        return false;
    };
    let Ok(relative) = path.strip_prefix(root) else {
        return false;
    };
    relative.components().next().is_some_and(|component| {
        matches!(
            component.as_os_str().to_string_lossy().as_ref(),
            "3d-generator" | "vectors" | "images"
        )
    })
}

pub(super) fn is_managed_artifact(path: &Path) -> bool {
    is_managed_artifact_under(path, &crate::paths::app_local_data_dir())
}

fn cleanup_matches_under(item: &PendingCleanup, path: &Path, managed_root: &Path) -> bool {
    is_managed_artifact_under(path, managed_root)
        && digest_file(path).is_ok_and(|(size, digest)| {
            size == item.artifact_size_bytes && digest == item.artifact_sha256
        })
        && crate::overlay::creation_file_identity::from_path(path)
            .is_ok_and(|identity| identity == item.artifact_file_identity)
}

fn lock_cleanup_artifact(path: &Path, item: &PendingCleanup) -> Option<std::fs::File> {
    crate::overlay::creation_delivery::publication::lock_owned_path(
        path,
        Some(&item.artifact_file_identity),
        false,
        true,
    )
    .ok()
}

pub(super) fn path_identity(path: &str) -> String {
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| PathBuf::from(path))
        .to_string_lossy()
        .to_ascii_lowercase()
}

fn path_exists_no_follow(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}

pub(super) fn prepare_cleanup_quarantines(store: &mut ResultHistoryStore) -> bool {
    let mut changed = false;
    let mut reserved = store
        .pending_cleanup
        .iter()
        .filter(|item| !item.quarantine_path.is_empty())
        .map(|item| path_identity(&item.quarantine_path))
        .collect::<std::collections::HashSet<_>>();
    for item in &mut store.pending_cleanup {
        if !item.quarantine_path.is_empty() {
            continue;
        }
        let Some(parent) = Path::new(&item.output_path).parent() else {
            continue;
        };
        for _ in 0..16 {
            let candidate = parent.join(format!(
                ".sgt-prune-{}-{}.tmp",
                std::process::id(),
                HISTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ));
            let identity = path_identity(&candidate.to_string_lossy());
            if !path_exists_no_follow(&candidate) && reserved.insert(identity) {
                item.quarantine_path = candidate.to_string_lossy().to_string();
                changed = true;
                break;
            }
        }
    }
    changed
}

fn valid_quarantine_path(item: &PendingCleanup) -> bool {
    let original = Path::new(&item.output_path);
    let quarantine = Path::new(&item.quarantine_path);
    !item.quarantine_path.is_empty()
        && original.is_absolute()
        && quarantine.is_absolute()
        && original.parent() == quarantine.parent()
        && quarantine
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(".sgt-prune-") && name.ends_with(".tmp"))
}

fn restore_modified_entry(
    store: &mut ResultHistoryStore,
    item: &PendingCleanup,
    restored_path: &Path,
) -> bool {
    let Some(mut entry) = item.history_entry.clone() else {
        return false;
    };
    let restored = restored_path.to_string_lossy().to_string();
    if store
        .entries
        .iter()
        .any(|current| same_path(&current.output_path, &restored))
    {
        return false;
    }
    let details = digest_file(restored_path).ok();
    entry.output_path = restored;
    entry.output_name = restored_path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or(entry.output_name);
    entry.artifact_size_bytes = details
        .as_ref()
        .map(|(size, _)| *size)
        .or_else(|| {
            std::fs::symlink_metadata(restored_path)
                .ok()
                .map(|item| item.len())
        })
        .unwrap_or(0);
    entry.artifact_sha256 = details.map(|(_, digest)| digest).unwrap_or_default();
    // Without an exact match to the committed digest, automatic cleanup
    // must treat these bytes as user-owned.
    entry.managed_artifact = false;
    store.entries.push(entry);
    true
}

fn recovered_path(item: &PendingCleanup) -> Option<PathBuf> {
    let original = Path::new(&item.output_path);
    let parent = original.parent()?;
    let stem = original
        .file_stem()
        .map(|value| value.to_string_lossy())
        .unwrap_or_else(|| "result".into());
    let extension = original
        .extension()
        .map(|value| format!(".{}", value.to_string_lossy()))
        .unwrap_or_default();
    for _ in 0..16 {
        let candidate = parent.join(format!(
            "{stem}-recovered-{}-{counter}{extension}",
            now_ms(),
            counter = HISTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed),
        ));
        if !path_exists_no_follow(&candidate) {
            return Some(candidate);
        }
    }
    None
}

fn resolution_target(item: &PendingCleanup) -> Option<&str> {
    match &item.resolution {
        CleanupResolution::Pending => None,
        CleanupResolution::Restore { target_path }
        | CleanupResolution::Relinquish { target_path } => Some(target_path),
    }
}

fn valid_resolution(item: &PendingCleanup, managed_root: &Path) -> bool {
    let original = Path::new(&item.output_path);
    match &item.resolution {
        CleanupResolution::Pending => true,
        CleanupResolution::Restore { target_path } => same_path(target_path, &item.output_path),
        CleanupResolution::Relinquish { target_path } => {
            let target = Path::new(target_path);
            let Some(parent) = original.parent() else {
                return false;
            };
            if !target.is_absolute() || target.parent() != Some(parent) {
                return false;
            }
            let (Ok(parent), Ok(root)) = (
                std::fs::canonicalize(parent),
                std::fs::canonicalize(managed_root),
            ) else {
                return false;
            };
            let managed_parent = parent
                .strip_prefix(root)
                .ok()
                .and_then(|relative| relative.components().next())
                .is_some_and(|component| {
                    matches!(
                        component.as_os_str().to_string_lossy().as_ref(),
                        "3d-generator" | "vectors" | "images"
                    )
                });
            let stem = original
                .file_stem()
                .map(|value| value.to_string_lossy())
                .unwrap_or_else(|| "result".into());
            let extension = original
                .extension()
                .map(|value| format!(".{}", value.to_string_lossy()))
                .unwrap_or_default();
            let prefix = format!("{stem}-recovered-");
            let Some(middle) = target
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| name.strip_prefix(&prefix))
                .and_then(|name| name.strip_suffix(&extension))
            else {
                return false;
            };
            managed_parent
                && middle.split_once('-').is_some_and(|(time, counter)| {
                    !time.is_empty()
                        && !counter.is_empty()
                        && time.bytes().all(|byte| byte.is_ascii_digit())
                        && counter.bytes().all(|byte| byte.is_ascii_digit())
                })
        }
    }
}

fn finish_cleanup_resolution(
    store: &mut ResultHistoryStore,
    index: usize,
    managed_root: &Path,
) -> bool {
    let item = store.pending_cleanup[index].clone();
    if !valid_resolution(&item, managed_root) {
        return false;
    }
    let Some(target_path) = resolution_target(&item) else {
        return false;
    };
    let target = Path::new(target_path);
    let quarantine = Path::new(&item.quarantine_path);
    if let Some(target_owned) = lock_cleanup_artifact(target, &item) {
        drop(target_owned);
        if quarantine != target
            && let Some(source_owned) = lock_cleanup_artifact(quarantine, &item)
            && crate::overlay::creation_delivery::publication::delete_owned(
                &source_owned,
                quarantine,
            )
            .is_err()
        {
            return false;
        }
    } else {
        let Some(source_owned) = lock_cleanup_artifact(quarantine, &item) else {
            return false;
        };
        if path_exists_no_follow(target)
            || crate::overlay::creation_delivery::publication::rename_owned_no_replace(
                &source_owned,
                quarantine,
                target,
            )
            .is_err()
        {
            return false;
        }
    }
    if lock_cleanup_artifact(target, &item).is_none() {
        return false;
    }
    restore_modified_entry(store, &item, target);
    store.pending_cleanup.remove(index);
    true
}

fn run_pending_cleanup_with(
    store: &mut ResultHistoryStore,
    managed_root: &Path,
    mut persist: impl FnMut(&ResultHistoryStore) -> Result<(), String>,
) -> Result<bool, String> {
    let previous_entries = store.entries.len();
    let previous_pending = store.pending_cleanup.len();
    let retained_paths = store
        .entries
        .iter()
        .map(|entry| path_identity(&entry.output_path))
        .collect::<std::collections::HashSet<_>>();
    let mut index = 0;
    while index < store.pending_cleanup.len() {
        let item = store.pending_cleanup[index].clone();
        if !valid_quarantine_path(&item) {
            store.pending_cleanup.remove(index);
            continue;
        }
        if !matches!(item.resolution, CleanupResolution::Pending) {
            if !finish_cleanup_resolution(store, index, managed_root) {
                index += 1;
            }
            continue;
        }
        let original = Path::new(&item.output_path);
        let quarantine = Path::new(&item.quarantine_path);
        if retained_paths.contains(&path_identity(&item.output_path))
            && !path_exists_no_follow(quarantine)
        {
            store.pending_cleanup.remove(index);
            continue;
        }
        if let Some(owned) = lock_cleanup_artifact(quarantine, &item) {
            if cleanup_matches_under(&item, quarantine, managed_root) {
                if crate::overlay::creation_delivery::publication::delete_owned(&owned, quarantine)
                    .is_err()
                {
                    index += 1;
                } else {
                    store.pending_cleanup.remove(index);
                }
                continue;
            }
            let resolution = if !path_exists_no_follow(original) {
                CleanupResolution::Restore {
                    target_path: item.output_path.clone(),
                }
            } else {
                let Some(target) = recovered_path(&item) else {
                    index += 1;
                    continue;
                };
                CleanupResolution::Relinquish {
                    target_path: target.to_string_lossy().to_string(),
                }
            };
            drop(owned);
            store.pending_cleanup[index].resolution = resolution;
            persist(store)?;
            if !finish_cleanup_resolution(store, index, managed_root) {
                index += 1;
            }
            continue;
        }
        if let Some(owned) = lock_cleanup_artifact(original, &item) {
            if !is_managed_artifact_under(original, managed_root) {
                index += 1;
                continue;
            }
            if cleanup_matches_under(&item, original, managed_root) {
                if crate::overlay::creation_delivery::publication::rename_owned_no_replace(
                    &owned, original, quarantine,
                )
                .is_err()
                {
                    index += 1;
                }
                continue;
            }
            store.pending_cleanup[index].resolution = CleanupResolution::Restore {
                target_path: item.output_path.clone(),
            };
            drop(owned);
            persist(store)?;
            if !finish_cleanup_resolution(store, index, managed_root) {
                index += 1;
            }
            continue;
        }
        if !path_exists_no_follow(original) && !path_exists_no_follow(quarantine) {
            store.pending_cleanup.remove(index);
        } else {
            index += 1;
        }
    }
    Ok(store.entries.len() != previous_entries || store.pending_cleanup.len() != previous_pending)
}

#[cfg(test)]
pub(super) fn run_pending_cleanup_under(
    store: &mut ResultHistoryStore,
    managed_root: &Path,
) -> bool {
    run_pending_cleanup_with(store, managed_root, |_| Ok(())).unwrap()
}

pub(super) fn run_pending_cleanup_at(
    path: &Path,
    store: &mut ResultHistoryStore,
) -> Result<bool, String> {
    run_pending_cleanup_with(store, &crate::paths::app_local_data_dir(), |store| {
        save_store(path, store)
    })
}

pub(super) fn queue_managed_cleanup(
    store: &mut ResultHistoryStore,
    entry: ResultHistoryEntry,
) -> bool {
    if !entry.managed_artifact || entry.artifact_size_bytes == 0 || entry.artifact_sha256.is_empty()
    {
        return false;
    }
    if let Some(cleanup) = companion::pending_cleanup(&entry)
        && !store.pending_cleanup.iter().any(|pending| {
            same_path(&pending.output_path, &cleanup.output_path)
                && pending.artifact_file_identity == cleanup.artifact_file_identity
        })
    {
        store.pending_cleanup.push(cleanup);
    }
    let artifact_file_identity = store
        .delivery_identities
        .iter()
        .find(|identity| identity.entry_id == entry.id)
        .map(|identity| identity.artifact_file_identity.clone())
        .unwrap_or_default();
    let cleanup = PendingCleanup {
        output_path: entry.output_path.clone(),
        artifact_size_bytes: entry.artifact_size_bytes,
        artifact_sha256: entry.artifact_sha256.clone(),
        artifact_file_identity,
        quarantine_path: String::new(),
        history_entry: Some(entry),
        resolution: CleanupResolution::Pending,
    };
    if !crate::overlay::creation_file_identity::valid(&cleanup.artifact_file_identity) {
        return false;
    }
    if store.pending_cleanup.iter().any(|pending| {
        same_path(&pending.output_path, &cleanup.output_path)
            && pending.artifact_size_bytes == cleanup.artifact_size_bytes
            && pending.artifact_sha256 == cleanup.artifact_sha256
            && pending.artifact_file_identity == cleanup.artifact_file_identity
    }) {
        return false;
    }
    store.pending_cleanup.push(cleanup);
    true
}

#[cfg(test)]
pub(super) fn prune_store(store: &mut ResultHistoryStore, results_per_tool: usize) -> bool {
    prune_store_protected(store, results_per_tool, &std::collections::HashSet::new())
}

pub(super) fn prune_store_protected(
    store: &mut ResultHistoryStore,
    results_per_tool: usize,
    protected_paths: &std::collections::HashSet<String>,
) -> bool {
    store
        .entries
        .sort_by_key(|entry| std::cmp::Reverse(entry.created_at_ms));
    let mut counts = std::collections::HashMap::<String, usize>::new();
    let mut pruned = Vec::new();
    store.entries.retain(|entry| {
        let count = counts.entry(entry.tool.clone()).or_default();
        *count += 1;
        if *count <= results_per_tool
            || protected_paths.contains(&path_identity(&entry.output_path))
        {
            true
        } else {
            pruned.push(entry.clone());
            false
        }
    });

    let mut newest_tools = std::collections::HashSet::new();
    let protected = store
        .entries
        .iter()
        .filter_map(|entry| {
            newest_tools
                .insert(entry.tool.clone())
                .then_some(entry.id.clone())
        })
        .collect::<std::collections::HashSet<_>>();
    let mut managed_bytes = store
        .entries
        .iter()
        .filter(|entry| entry.managed_artifact)
        .map(|entry| u128::from(entry.artifact_size_bytes))
        .chain(
            store
                .pending_cleanup
                .iter()
                .map(|entry| u128::from(entry.artifact_size_bytes)),
        )
        .fold(0_u128, u128::saturating_add);
    let mut index = store.entries.len();
    while index > 0 && managed_bytes > u128::from(MAX_MANAGED_ARTIFACT_BYTES) {
        index -= 1;
        let entry = &store.entries[index];
        if !entry.managed_artifact
            || protected.contains(&entry.id)
            || protected_paths.contains(&path_identity(&entry.output_path))
        {
            continue;
        }
        managed_bytes = managed_bytes.saturating_sub(u128::from(entry.artifact_size_bytes));
        pruned.push(store.entries.remove(index));
    }

    for entry in pruned.iter().cloned() {
        queue_managed_cleanup(store, entry);
    }
    !pruned.is_empty()
}

pub(super) fn reconcile_store(
    path: &Path,
    store: &mut ResultHistoryStore,
    results_per_tool: usize,
) -> Result<(), String> {
    reconcile_store_protected(
        path,
        store,
        results_per_tool,
        &std::collections::HashSet::new(),
    )
}

pub(super) fn reconcile_store_protected(
    path: &Path,
    store: &mut ResultHistoryStore,
    results_per_tool: usize,
    protected_paths: &std::collections::HashSet<String>,
) -> Result<(), String> {
    let mut changed = reconcile_pending_renames(store);
    let previous_len = store.entries.len();
    store.entries.retain(|entry| {
        std::fs::symlink_metadata(&entry.output_path)
            .map(|metadata| metadata.file_type().is_file())
            .unwrap_or(false)
    });
    changed |= store.entries.len() != previous_len;
    changed |= prune_store_protected(store, results_per_tool, protected_paths);
    changed |= retain_live_delivery_identities(store);
    changed |= prepare_cleanup_quarantines(store);
    if changed {
        save_store(path, store)?;
    }
    Ok(())
}
