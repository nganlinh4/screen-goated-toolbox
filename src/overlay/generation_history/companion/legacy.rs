//! One-way migration of companion state written by older history versions.
//!
//! Older renames changed only the primary name, and older retention journaled
//! each pair member as an independent cleanup. Both are migrated only when the
//! binding is unambiguous; anything else is left untouched for the user.

use std::time::UNIX_EPOCH;

use super::super::deletion::replay::{self, Probe, Role};
use super::*;

/// A companion is never newer than the history record that published it.
const RECORD_CLOCK_SKEW_MS: u128 = 60_000;

fn references(entry: &ResultHistoryEntry, path: &str) -> bool {
    same_path(&entry.output_path, path)
        || entry
            .metadata
            .get("download")
            .and_then(|value| value.get("path"))
            .and_then(Value::as_str)
            .is_some_and(|side| same_path(side, path))
}

/// Whether any live entry other than `owner`, or any journal, names a path.
fn referenced_elsewhere(store: &ResultHistoryStore, owner: Option<usize>, paths: &[&str]) -> bool {
    paths.iter().any(|path| {
        store
            .entries
            .iter()
            .enumerate()
            .any(|(index, entry)| Some(index) != owner && references(entry, path))
            || store
                .pending_deletes
                .iter()
                .any(|pending| references(&pending.entry, path))
            || store.pending_renames.iter().any(|rename| {
                same_path(&rename.previous_path, path) || same_path(&rename.next_path, path)
            })
    })
}

fn exact(path: &Path, side: &CompanionIdentity) -> Option<std::fs::File> {
    match replay::probe(path, side.size_bytes, &side.sha256, &side.file_identity) {
        Probe::Exact(file) => Some(file),
        _ => None,
    }
}

/// Bind companions of entries renamed before the original stem was recorded.
pub(in crate::overlay::generation_history) fn bind_orphaned(
    store: &mut ResultHistoryStore,
) -> bool {
    let mut changed = false;
    for index in 0..store.entries.len() {
        if let Some(origin) = orphan_origin(store, index) {
            store.entries[index].companion_origin_path = Some(origin);
            changed = true;
        }
    }
    changed
}

fn orphan_origin(store: &ResultHistoryStore, index: usize) -> Option<String> {
    let entry = &store.entries[index];
    if entry.companion_origin_path.is_some() || from_entry(entry).is_some() {
        return None;
    }
    let side: CompanionIdentity =
        serde_json::from_value(entry.metadata.get("download")?.clone()).ok()?;
    let origin = Path::new(&side.path)
        .with_extension(Path::new(&entry.output_path).extension()?)
        .to_string_lossy()
        .into_owned();
    let mut bound = entry.clone();
    bound.companion_origin_path = Some(origin.clone());
    // Every structural rule except the primary's current stem must hold.
    from_entry(&bound)?;
    if referenced_elsewhere(store, Some(index), &[&side.path, &origin])
        || store.pending_cleanup.iter().any(|item| {
            same_path(&item.output_path, &side.path)
                || item
                    .history_entry
                    .as_ref()
                    .is_some_and(|other| references(other, &side.path))
        })
    {
        return None;
    }
    let modified = std::fs::symlink_metadata(&side.path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_millis();
    if modified > u128::from(entry.created_at_ms) + RECORD_CLOCK_SKEW_MS {
        return None;
    }
    exact(Path::new(&side.path), &side).map(|_| origin)
}

fn legacy_quarantine(item: &PendingCleanup) -> Option<&Path> {
    let original = Path::new(&item.output_path);
    let quarantine = Path::new(&item.quarantine_path);
    (!item.quarantine_path.is_empty()
        && quarantine.is_absolute()
        && quarantine.parent() == original.parent()
        && quarantine
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(".sgt-prune-") && name.ends_with(".tmp")))
    .then_some(quarantine)
}

fn matches_side(item: &PendingCleanup, side: &CompanionIdentity) -> bool {
    item.artifact_size_bytes == side.size_bytes
        && item.artifact_sha256 == side.sha256
        && item.artifact_file_identity == side.file_identity
        && matches!(item.resolution, CleanupResolution::Pending)
}

/// Move one legacy member to the pair-deletion quarantine. `false` means the
/// member is in a state this migration must not touch.
fn adopt_member(
    original: &Path,
    legacy: Option<&Path>,
    target: &Path,
    side: &CompanionIdentity,
) -> bool {
    match replay::probe(target, side.size_bytes, &side.sha256, &side.file_identity) {
        Probe::Exact(_) => return true,
        Probe::Absent => {}
        Probe::Different | Probe::Unavailable => return false,
    }
    if let Some(legacy) = legacy {
        match replay::probe(legacy, side.size_bytes, &side.sha256, &side.file_identity) {
            Probe::Exact(file) => {
                return crate::overlay::creation_delivery::publication::rename_owned_no_replace(
                    &file, legacy, target,
                )
                .is_ok();
            }
            Probe::Absent => {}
            Probe::Different | Probe::Unavailable => return false,
        }
    }
    // A visible member is classified again, under the pair journal, by replay.
    !matches!(
        replay::probe(original, side.size_bytes, &side.sha256, &side.file_identity),
        Probe::Unavailable
    )
}

/// Convert an independent legacy pair cleanup into one pair-deletion journal.
fn migrate_pair(
    store: &ResultHistoryStore,
    index: usize,
    protected: &protected::ProtectedPaths,
) -> Option<(Vec<usize>, deletion::PendingDelete)> {
    let item = &store.pending_cleanup[index];
    let entry = item.history_entry.as_ref()?;
    let side = checked_from_entry(entry).ok()??;
    let primary = CompanionIdentity {
        path: entry.output_path.clone(),
        name: entry.output_name.clone(),
        size_bytes: item.artifact_size_bytes,
        sha256: item.artifact_sha256.clone(),
        file_identity: item.artifact_file_identity.clone(),
    };
    if !matches!(item.resolution, CleanupResolution::Pending)
        || !same_path(&item.output_path, &entry.output_path)
        || item.artifact_size_bytes != entry.artifact_size_bytes
        || item.artifact_sha256 != entry.artifact_sha256
        || !crate::overlay::creation_file_identity::valid(&item.artifact_file_identity)
        || protected.conflicts_entry(entry).unwrap_or(true)
        || referenced_elsewhere(store, None, &[&entry.output_path, &side.path])
    {
        return None;
    }
    let mut side_items = Vec::new();
    for (other_index, other) in store.pending_cleanup.iter().enumerate() {
        if other_index == index {
            continue;
        }
        if same_path(&other.output_path, &entry.output_path)
            || other
                .history_entry
                .as_ref()
                .is_some_and(|other| references(other, &side.path))
        {
            return None;
        }
        if same_path(&other.output_path, &side.path) {
            if other.history_entry.is_some() || !matches_side(other, &side) {
                return None;
            }
            side_items.push(other_index);
        }
    }
    if side_items.len() > 1 {
        return None;
    }
    let side_legacy = side_items
        .first()
        .and_then(|&other| legacy_quarantine(&store.pending_cleanup[other]));
    for (member, legacy, role) in [
        (&primary, legacy_quarantine(item), Role::Primary),
        (&side, side_legacy, Role::Companion),
    ] {
        let original = Path::new(&member.path);
        let target = replay::quarantine_path(entry, original, role)?;
        if legacy.is_some_and(|path| protected.conflicts_path(path).unwrap_or(true))
            || !adopt_member(original, legacy, &target, member)
        {
            return None;
        }
    }
    let mut remove = side_items;
    remove.push(index);
    Some((
        remove,
        deletion::PendingDelete::new(entry.clone(), item.artifact_file_identity.clone()),
    ))
}

/// Return a legacy companion cleanup to the live entry that still owns it.
fn release_owned_companion(store: &ResultHistoryStore, index: usize) -> bool {
    let item = &store.pending_cleanup[index];
    if item.history_entry.is_some() {
        return false;
    }
    let owners = store
        .entries
        .iter()
        .filter_map(|entry| checked_from_entry(entry).ok().flatten())
        .filter(|side| same_path(&side.path, &item.output_path))
        .collect::<Vec<_>>();
    let [side] = owners.as_slice() else {
        return false;
    };
    if !matches_side(item, side)
        || store
            .pending_cleanup
            .iter()
            .enumerate()
            .any(|(other, cleanup)| {
                other != index
                    && (same_path(&cleanup.output_path, &side.path)
                        || cleanup
                            .history_entry
                            .as_ref()
                            .is_some_and(|entry| references(entry, &side.path)))
            })
    {
        return false;
    }
    let original = Path::new(&side.path);
    if let Some(legacy) = legacy_quarantine(item)
        && let Probe::Exact(file) =
            replay::probe(legacy, side.size_bytes, &side.sha256, &side.file_identity)
    {
        return matches!(
            replay::probe(original, side.size_bytes, &side.sha256, &side.file_identity),
            Probe::Absent
        ) && crate::overlay::creation_delivery::publication::rename_owned_no_replace(
            &file, legacy, original,
        )
        .is_ok();
    }
    exact(original, side).is_some()
}

pub(in crate::overlay::generation_history) fn migrate_cleanup(
    store: &mut ResultHistoryStore,
    protected: &protected::ProtectedPaths,
) -> bool {
    let mut changed = false;
    let mut index = 0;
    while index < store.pending_cleanup.len() {
        if let Some((mut remove, pending)) = migrate_pair(store, index, protected) {
            remove.sort_unstable_by(|left, right| right.cmp(left));
            for item in remove {
                store.pending_cleanup.remove(item);
            }
            store.pending_deletes.push(pending);
            changed = true;
            index = 0;
            continue;
        }
        if !protected
            .conflicts_path(Path::new(&store.pending_cleanup[index].output_path))
            .unwrap_or(true)
            && release_owned_companion(store, index)
        {
            store.pending_cleanup.remove(index);
            changed = true;
            continue;
        }
        index += 1;
    }
    changed
}
