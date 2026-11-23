use super::*;

fn path_exists_no_follow(path: &Path) -> std::io::Result<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let parent = path.parent().ok_or(error)?;
            match std::fs::metadata(parent) {
                Ok(metadata) if metadata.is_dir() => Ok(false),
                Ok(_) => Err(std::io::Error::new(
                    std::io::ErrorKind::NotADirectory,
                    "cleanup parent is not a directory",
                )),
                Err(error) => Err(error),
            }
        }
        Err(error) => Err(error),
    }
}

pub(in crate::overlay::generation_history) fn prepare_cleanup_quarantines(
    store: &mut ResultHistoryStore,
) -> bool {
    prepare_cleanup_quarantines_with(store, &path_exists_no_follow)
}

pub(in crate::overlay::generation_history) fn prepare_cleanup_quarantines_with(
    store: &mut ResultHistoryStore,
    exists: &dyn Fn(&Path) -> std::io::Result<bool>,
) -> bool {
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
            if matches!(exists(&candidate), Ok(false)) && reserved.insert(identity) {
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
) -> Result<bool, ()> {
    let Some(mut entry) = item.history_entry.clone() else {
        return Ok(false);
    };
    let restored = restored_path.to_string_lossy().to_string();
    if store
        .entries
        .iter()
        .any(|current| same_path(&current.output_path, &restored))
    {
        return Ok(false);
    }
    let (size, digest) = digest_file(restored_path).map_err(|_| ())?;
    entry.output_path = restored;
    entry.output_name = restored_path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or(entry.output_name);
    entry.artifact_size_bytes = size;
    entry.artifact_sha256 = digest;
    // Without an exact match to the committed digest, automatic cleanup
    // must treat these bytes as user-owned.
    entry.managed_artifact = false;
    store.entries.push(entry);
    Ok(true)
}

fn recovered_path(
    item: &PendingCleanup,
    exists: &dyn Fn(&Path) -> std::io::Result<bool>,
) -> Option<PathBuf> {
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
        if matches!(exists(&candidate), Ok(false)) {
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
    exists: &dyn Fn(&Path) -> std::io::Result<bool>,
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
    if exists(target).is_err() || exists(quarantine).is_err() {
        return false;
    }
    if let Some(target_owned) = lock_cleanup_artifact(target, &item) {
        drop(target_owned);
        if quarantine != target {
            match exists(quarantine) {
                Ok(false) => {}
                Ok(true) => {
                    let Some(source_owned) = lock_cleanup_artifact(quarantine, &item) else {
                        return false;
                    };
                    if crate::overlay::creation_delivery::publication::delete_owned(
                        &source_owned,
                        quarantine,
                    )
                    .is_err()
                    {
                        return false;
                    }
                }
                Err(_) => return false,
            }
        }
    } else {
        let Some(source_owned) = lock_cleanup_artifact(quarantine, &item) else {
            return false;
        };
        if !matches!(exists(target), Ok(false))
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
    let Some(_target_guard) = lock_cleanup_artifact(target, &item) else {
        return false;
    };
    if restore_modified_entry(store, &item, target).is_err() {
        return false;
    }
    store.pending_cleanup.remove(index);
    true
}

fn run_pending_cleanup_with(
    store: &mut ResultHistoryStore,
    managed_root: &Path,
    mut persist: impl FnMut(&ResultHistoryStore) -> Result<(), String>,
    exists: &dyn Fn(&Path) -> std::io::Result<bool>,
    protected: &protected::ProtectedPaths,
) -> Result<bool, String> {
    let previous_entries = store.entries.len();
    let previous_pending = store.pending_cleanup.len();
    let retained_paths = store
        .entries
        .iter()
        .map(|entry| path_identity(&entry.output_path))
        .collect::<std::collections::HashSet<_>>();
    // Old journals recorded a pair as independent operations, which cannot
    // safely prove group ownership after either member moved to quarantine.
    let mut legacy_pair_paths = Vec::new();
    for item in &store.pending_cleanup {
        let Some(entry) = &item.history_entry else {
            continue;
        };
        if entry
            .metadata
            .get("download")
            .is_some_and(|value| !value.is_null())
        {
            legacy_pair_paths.push(item.output_path.clone());
            if let Some(path) = entry
                .metadata
                .get("download")
                .and_then(|value| value.get("path"))
                .and_then(Value::as_str)
            {
                legacy_pair_paths.push(path.to_string());
            }
        }
    }
    let mut index = 0;
    while index < store.pending_cleanup.len() {
        let item = store.pending_cleanup[index].clone();
        if protected
            .conflicts_path(Path::new(&item.output_path))
            .unwrap_or(true)
            || (!item.quarantine_path.is_empty()
                && protected
                    .conflicts_path(Path::new(&item.quarantine_path))
                    .unwrap_or(true))
        {
            index += 1;
            continue;
        }
        if item.history_entry.is_none()
            || legacy_pair_paths
                .iter()
                .any(|path| same_path(path, &item.output_path))
        {
            index += 1;
            continue;
        }
        // Reservation can be deferred when the directory cannot be inspected.
        // Keep the durable intent until a later pass can reserve a path.
        if item.quarantine_path.is_empty() {
            index += 1;
            continue;
        }
        if !valid_quarantine_path(&item) {
            store.pending_cleanup.remove(index);
            continue;
        }
        if !matches!(item.resolution, CleanupResolution::Pending) {
            if !finish_cleanup_resolution(store, index, managed_root, exists) {
                index += 1;
            }
            continue;
        }
        let original = Path::new(&item.output_path);
        let quarantine = Path::new(&item.quarantine_path);
        if exists(original).is_err() || exists(quarantine).is_err() {
            index += 1;
            continue;
        }
        if retained_paths.contains(&path_identity(&item.output_path))
            && matches!(exists(quarantine), Ok(false))
        {
            store.pending_cleanup.remove(index);
            continue;
        }
        if let Some(owned) = lock_cleanup_artifact(quarantine, &item) {
            let matches = match cleanup_matches_under(&item, quarantine, managed_root) {
                Ok(matches) => matches,
                Err(()) => {
                    index += 1;
                    continue;
                }
            };
            if matches {
                if crate::overlay::creation_delivery::publication::delete_owned(&owned, quarantine)
                    .is_err()
                {
                    index += 1;
                } else {
                    store.pending_cleanup.remove(index);
                }
                continue;
            }
            let resolution = match exists(original) {
                Ok(false) => CleanupResolution::Restore {
                    target_path: item.output_path.clone(),
                },
                Ok(true) => {
                    let Some(target) = recovered_path(&item, exists) else {
                        index += 1;
                        continue;
                    };
                    CleanupResolution::Relinquish {
                        target_path: target.to_string_lossy().to_string(),
                    }
                }
                Err(_) => {
                    index += 1;
                    continue;
                }
            };
            drop(owned);
            store.pending_cleanup[index].resolution = resolution;
            persist(store)?;
            if !finish_cleanup_resolution(store, index, managed_root, exists) {
                index += 1;
            }
            continue;
        }
        if let Some(owned) = lock_cleanup_artifact(original, &item) {
            if !is_managed_artifact_under(original, managed_root) {
                index += 1;
                continue;
            }
            let matches = match cleanup_matches_under(&item, original, managed_root) {
                Ok(matches) => matches,
                Err(()) => {
                    index += 1;
                    continue;
                }
            };
            if matches {
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
            if !finish_cleanup_resolution(store, index, managed_root, exists) {
                index += 1;
            }
            continue;
        }
        if matches!(exists(original), Ok(false)) && matches!(exists(quarantine), Ok(false)) {
            store.pending_cleanup.remove(index);
        } else {
            index += 1;
        }
    }
    Ok(store.entries.len() != previous_entries || store.pending_cleanup.len() != previous_pending)
}

#[cfg(test)]
pub(in crate::overlay::generation_history) fn run_pending_cleanup_under(
    store: &mut ResultHistoryStore,
    managed_root: &Path,
) -> bool {
    run_pending_cleanup_with(
        store,
        managed_root,
        |_| Ok(()),
        &path_exists_no_follow,
        &protected::ProtectedPaths::default(),
    )
    .unwrap()
}

#[cfg(test)]
pub(in crate::overlay::generation_history) fn run_pending_cleanup_under_with(
    store: &mut ResultHistoryStore,
    managed_root: &Path,
    exists: &dyn Fn(&Path) -> std::io::Result<bool>,
) -> bool {
    run_pending_cleanup_with(
        store,
        managed_root,
        |_| Ok(()),
        exists,
        &protected::ProtectedPaths::default(),
    )
    .unwrap()
}

pub(in crate::overlay::generation_history) fn run_pending_cleanup_at_protected(
    path: &Path,
    store: &mut ResultHistoryStore,
    protected: &protected::ProtectedPaths,
) -> Result<bool, String> {
    run_pending_cleanup_with(
        store,
        &crate::paths::app_runtime_local_data_dir(),
        |store| save_store(path, store),
        &path_exists_no_follow,
        protected,
    )
}
