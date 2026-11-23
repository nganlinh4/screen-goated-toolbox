use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use sha2::{Digest as _, Sha256};

use super::*;

mod cleanup;
pub(super) use cleanup::{prepare_cleanup_quarantines, run_pending_cleanup_at_protected};
#[cfg(test)]
pub(super) use cleanup::{
    prepare_cleanup_quarantines_with, run_pending_cleanup_under, run_pending_cleanup_under_with,
};

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
    is_managed_artifact_under(path, &crate::paths::app_runtime_local_data_dir())
}

fn cleanup_matches_under(
    item: &PendingCleanup,
    path: &Path,
    managed_root: &Path,
) -> Result<bool, ()> {
    if !is_managed_artifact_under(path, managed_root) {
        return Err(());
    }
    let (size, digest) = digest_file(path).map_err(|_| ())?;
    let identity = crate::overlay::creation_file_identity::from_path(path).map_err(|_| ())?;
    Ok(size == item.artifact_size_bytes
        && digest == item.artifact_sha256
        && identity == item.artifact_file_identity)
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

pub(super) fn queue_managed_cleanup(
    store: &mut ResultHistoryStore,
    entry: ResultHistoryEntry,
) -> bool {
    if !entry.managed_artifact || entry.artifact_size_bytes == 0 || entry.artifact_sha256.is_empty()
    {
        return false;
    }
    match companion::checked_from_entry(&entry) {
        Err(()) => return false,
        Ok(Some(_)) => {
            let Some(pending) = deletion::retention_pair(store, &entry) else {
                return false;
            };
            if store
                .pending_deletes
                .iter()
                .any(|existing| existing.entry.id == entry.id)
            {
                return false;
            }
            store.pending_deletes.push(pending);
            return true;
        }
        Ok(None) => {}
    }
    let artifact_file_identity = store
        .delivery_identities
        .iter()
        .find(|identity| identity.entry_id == entry.id)
        .map(|identity| identity.artifact_file_identity.clone())
        .unwrap_or_default();
    if !crate::overlay::creation_file_identity::valid(&artifact_file_identity) {
        return false;
    }
    let cleanup = PendingCleanup {
        output_path: entry.output_path.clone(),
        artifact_size_bytes: entry.artifact_size_bytes,
        artifact_sha256: entry.artifact_sha256.clone(),
        artifact_file_identity,
        quarantine_path: String::new(),
        history_entry: Some(entry),
        resolution: CleanupResolution::Pending,
    };
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

pub(super) fn retention_eligible(store: &ResultHistoryStore, entry: &ResultHistoryEntry) -> bool {
    match companion::checked_from_entry(entry) {
        Ok(None) => true,
        Ok(Some(_)) => deletion::retention_pair(store, entry).is_some(),
        Err(()) => false,
    }
}

fn retained_artifact_bytes(entry: &ResultHistoryEntry) -> u128 {
    let side_bytes = companion::checked_from_entry(entry)
        .ok()
        .flatten()
        .and_then(|side| {
            let file = crate::overlay::creation_delivery::publication::lock_owned_path(
                Path::new(&side.path),
                Some(&side.file_identity),
                false,
                false,
            )
            .ok()?;
            let size = file.metadata().ok()?.len();
            (size == side.size_bytes).then_some(size)
        })
        .unwrap_or(0);
    u128::from(entry.artifact_size_bytes).saturating_add(u128::from(side_bytes))
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
    let pending_pair_bytes = store
        .pending_deletes
        .iter()
        .filter(|item| item.entry.managed_artifact)
        .map(|item| retained_artifact_bytes(&item.entry))
        .fold(0_u128, u128::saturating_add);
    let initial_bytes = store
        .entries
        .iter()
        .filter(|entry| entry.managed_artifact)
        .map(retained_artifact_bytes)
        .chain(
            store
                .pending_cleanup
                .iter()
                .map(|item| u128::from(item.artifact_size_bytes)),
        )
        .fold(pending_pair_bytes, u128::saturating_add);
    let under_pressure = initial_bytes > u128::from(MAX_MANAGED_ARTIFACT_BYTES);
    let mut candidate_counts = std::collections::HashMap::<&str, usize>::new();
    let ineligible = store
        .entries
        .iter()
        .filter(|entry| {
            let count = candidate_counts.entry(entry.tool.as_str()).or_default();
            *count += 1;
            (*count > results_per_tool || (under_pressure && entry.managed_artifact))
                && !retention_eligible(store, entry)
        })
        .map(|entry| entry.id.clone())
        .collect::<std::collections::HashSet<_>>();
    let pending = &store.pending_renames;
    let mut counts = std::collections::HashMap::<String, usize>::new();
    let mut pruned = Vec::new();
    store.entries.retain(|entry| {
        let count = counts.entry(entry.tool.clone()).or_default();
        *count += 1;
        if *count <= results_per_tool
            || ineligible.contains(&entry.id)
            || pending
                .iter()
                .any(|rename| rename.tool == entry.tool && rename.entry_id == entry.id)
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
        .map(retained_artifact_bytes)
        .chain(
            store
                .pending_cleanup
                .iter()
                .map(|entry| u128::from(entry.artifact_size_bytes)),
        )
        .fold(pending_pair_bytes, u128::saturating_add);
    let mut index = store.entries.len();
    while index > 0 && managed_bytes > u128::from(MAX_MANAGED_ARTIFACT_BYTES) {
        index -= 1;
        let entry = &store.entries[index];
        if !entry.managed_artifact
            || ineligible.contains(&entry.id)
            || protected.contains(&entry.id)
            || pending
                .iter()
                .any(|rename| rename.tool == entry.tool && rename.entry_id == entry.id)
            || protected_paths.contains(&path_identity(&entry.output_path))
        {
            continue;
        }
        managed_bytes = managed_bytes.saturating_sub(retained_artifact_bytes(entry));
        pruned.push(store.entries.remove(index));
    }

    for entry in pruned.iter().cloned() {
        if !queue_managed_cleanup(store, entry.clone())
            && !matches!(companion::checked_from_entry(&entry), Ok(None))
        {
            store.entries.push(entry);
        }
    }
    !pruned.is_empty()
}

pub(super) fn reconcile_store_protected(
    path: &Path,
    store: &mut ResultHistoryStore,
    results_per_tool: usize,
    protected_paths: &std::collections::HashSet<String>,
) -> Result<(), String> {
    let protected =
        protected::ProtectedPaths::from_saved_paths(protected_paths.iter().map(PathBuf::from))?;
    let mut changed = reconcile_pending_renames(store, &protected);
    changed |= companion::bind_orphaned(store);
    let previous_len = store.entries.len();
    let pending = &store.pending_renames;
    store.entries.retain(|entry| {
        pending
            .iter()
            .any(|rename| rename.tool == entry.tool && rename.entry_id == entry.id)
            || !matches!(companion::checked_from_entry(entry), Ok(None))
            || super::snapshot::output_may_exist(Path::new(&entry.output_path))
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
