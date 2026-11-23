use super::*;

pub(super) mod replay;

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PendingDelete {
    pub(super) entry: ResultHistoryEntry,
    artifact_file_identity: String,
}

impl PendingDelete {
    pub(super) fn new(entry: ResultHistoryEntry, artifact_file_identity: String) -> Self {
        Self {
            entry,
            artifact_file_identity,
        }
    }
}

fn pending_path(store: &ResultHistoryStore, path: &str) -> bool {
    if store.pending_deletes.is_empty() {
        return false;
    }
    let canonical = path_identity(path);
    let Ok(identity) = crate::overlay::creation_file_identity::from_path(Path::new(path)) else {
        return true;
    };
    store.pending_deletes.iter().any(|pending| {
        if canonical == path_identity(&pending.entry.output_path)
            || identity == pending.artifact_file_identity
        {
            return true;
        }
        match companion::checked_from_entry(&pending.entry) {
            Ok(Some(side)) => {
                canonical == path_identity(&side.path) || identity == side.file_identity
            }
            Ok(None) => false,
            Err(()) => true,
        }
    })
}

pub(super) fn pending_admission(store: &ResultHistoryStore, path: &str, metadata: &Value) -> bool {
    if pending_path(store, path) {
        return true;
    }
    if store.pending_deletes.is_empty() {
        return false;
    }
    match metadata.get("download") {
        None | Some(Value::Null) => false,
        Some(value) => value
            .get("path")
            .and_then(Value::as_str)
            .is_none_or(|side| pending_path(store, side)),
    }
}

pub(super) fn retention_pair(
    store: &ResultHistoryStore,
    entry: &ResultHistoryEntry,
) -> Option<PendingDelete> {
    let side = companion::checked_from_entry(entry).ok()??;
    let identity = store
        .delivery_identities
        .iter()
        .find(|identity| identity.tool == entry.tool && identity.entry_id == entry.id)?
        .artifact_file_identity
        .clone();
    let pending = PendingDelete {
        entry: entry.clone(),
        artifact_file_identity: identity,
    };
    let primary = owned_for_delete(
        &entry.output_path,
        entry.artifact_size_bytes,
        &entry.artifact_sha256,
        &pending.artifact_file_identity,
    )
    .ok()??;
    let secondary = owned_for_delete(
        &side.path,
        side.size_bytes,
        &side.sha256,
        &side.file_identity,
    )
    .ok()??;
    let reservation = ResultHistoryStore {
        pending_deletes: vec![pending.clone()],
        ..Default::default()
    };
    if store.entries.iter().any(|other| {
        other.id != entry.id && pending_admission(&reservation, &other.output_path, &other.metadata)
    }) {
        return None;
    }
    drop((primary, secondary));
    Some(pending)
}

fn owned_for_delete(
    path: &str,
    size: u64,
    digest: &str,
    identity: &str,
) -> Result<Option<std::fs::File>, ()> {
    let path = Path::new(path);
    if !path.is_absolute()
        || digest.len() != 64
        || !crate::overlay::creation_file_identity::valid(identity)
    {
        return Err(());
    }
    match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let parent = path.parent().ok_or(())?;
            return std::fs::metadata(parent)
                .map_err(|_| ())
                .and_then(|metadata| if metadata.is_dir() { Ok(None) } else { Err(()) });
        }
        Err(_) => return Err(()),
        Ok(_) => {}
    }
    let file = crate::overlay::creation_delivery::publication::lock_owned_path(
        path,
        Some(identity),
        false,
        true,
    )
    .map_err(|_| ())?;
    let artifact = inspect_delivery_artifact(&path.to_string_lossy()).map_err(|_| ())?;
    if artifact.size_bytes != size
        || artifact.sha256 != digest
        || artifact.file_identity != identity
    {
        return Err(());
    }
    Ok(Some(file))
}

pub(super) fn reclaimable_primary(store: &ResultHistoryStore, entry: &ResultHistoryEntry) -> bool {
    let Some(identity) = store
        .delivery_identities
        .iter()
        .find(|identity| identity.tool == entry.tool && identity.entry_id == entry.id)
    else {
        return false;
    };
    let Ok(Some(_owned)) = owned_for_delete(
        &entry.output_path,
        entry.artifact_size_bytes,
        &entry.artifact_sha256,
        &identity.artifact_file_identity,
    ) else {
        return false;
    };
    let reservation = ResultHistoryStore {
        pending_deletes: vec![PendingDelete {
            entry: entry.clone(),
            artifact_file_identity: identity.artifact_file_identity.clone(),
        }],
        ..Default::default()
    };
    !store.entries.iter().any(|other| {
        other.id != entry.id && pending_admission(&reservation, &other.output_path, &other.metadata)
    })
}

#[cfg(test)]
pub(super) fn reconcile_pending_deletes(store: &mut ResultHistoryStore) -> bool {
    reconcile_pending_deletes_protected(store, &protected::ProtectedPaths::default())
}

pub(super) fn reconcile_pending_deletes_protected(
    store: &mut ResultHistoryStore,
    protected: &protected::ProtectedPaths,
) -> bool {
    let mut changed = false;
    for pending in std::mem::take(&mut store.pending_deletes) {
        if protected.conflicts_entry(&pending.entry).unwrap_or(true) {
            store.pending_deletes.push(pending);
            continue;
        }
        match replay::execute(&pending) {
            replay::Replay::Retry => store.pending_deletes.push(pending),
            replay::Replay::Deleted | replay::Replay::Restored(None) => changed = true,
            replay::Replay::Restored(Some(entry)) => {
                if !store
                    .entries
                    .iter()
                    .any(|other| other.tool == entry.tool && other.id == entry.id)
                {
                    store.entries.push(*entry);
                }
                changed = true;
            }
        }
    }
    if changed {
        retain_live_delivery_identities(store);
    }
    changed
}

#[cfg(test)]
pub(super) fn delete_at(path: &Path, tool: &str, id: &str) -> Result<(), CreationError> {
    delete_at_protected(path, tool, id, &protected::ProtectedPaths::default())
}

pub(super) fn delete_at_protected(
    path: &Path,
    tool: &str,
    id: &str,
    protected: &protected::ProtectedPaths,
) -> Result<(), CreationError> {
    validate_tool(tool)?;
    let mut store = load_store(path)?;
    if reconcile_pending_renames(&mut store, protected) {
        save_store(path, &store)?;
    }
    if store
        .pending_renames
        .iter()
        .any(|rename| rename.tool == tool && rename.entry_id == id)
    {
        return Err(rename::busy("Result rename is awaiting recovery."));
    }
    let index = store
        .entries
        .iter()
        .position(|entry| entry.tool == tool && entry.id == id)
        .ok_or_else(|| "Result is no longer in history.".to_string())?;
    if protected.conflicts_entry(&store.entries[index])? {
        return Err(rename::busy("Result is in use by a pending delivery."));
    }
    let has_proof = store.delivery_identities.iter().any(|identity| {
        identity.tool == tool
            && identity.entry_id == id
            && crate::overlay::creation_file_identity::valid(&identity.artifact_file_identity)
    }) && store.entries[index].artifact_sha256.len() == 64;
    if !has_proof {
        store.entries.remove(index);
        retain_live_delivery_identities(&mut store);
        return Ok(save_store(path, &store)?);
    }
    let identity = store
        .delivery_identities
        .iter()
        .find(|identity| identity.tool == tool && identity.entry_id == id)
        .unwrap()
        .artifact_file_identity
        .clone();
    let pending = PendingDelete {
        entry: store.entries[index].clone(),
        artifact_file_identity: identity,
    };
    let reservation = ResultHistoryStore {
        pending_deletes: vec![pending.clone()],
        ..Default::default()
    };
    if store
        .entries
        .iter()
        .enumerate()
        .any(|(other_index, other)| {
            other_index != index
                && pending_admission(&reservation, &other.output_path, &other.metadata)
        })
    {
        return Err("Result files are shared by another history entry.".into());
    }
    // Verify both members before durably accepting the operation.
    let primary = owned_for_delete(
        &pending.entry.output_path,
        pending.entry.artifact_size_bytes,
        &pending.entry.artifact_sha256,
        &pending.artifact_file_identity,
    )
    .map_err(|_| "Result file is unavailable or changed.".to_string())?;
    companion::checked_from_entry(&pending.entry)
        .map_err(|_| "Result companion ownership cannot be verified.".to_string())?;
    let side = companion::lock_for_entry(&pending.entry)?;
    drop((primary, side));
    store.entries.remove(index);
    store.pending_deletes.push(pending);
    save_store(path, &store)?;
    if reconcile_pending_deletes_protected(&mut store, protected) {
        save_store(path, &store)?;
    }
    if store
        .pending_deletes
        .iter()
        .any(|pending| pending.entry.tool == tool && pending.entry.id == id)
    {
        Err(rename::busy("Result deletion is awaiting recovery."))
    } else {
        Ok(())
    }
}

pub(super) fn lock_recorded_artifact(
    store: &ResultHistoryStore,
    index: usize,
) -> Result<std::fs::File, String> {
    let entry = &store.entries[index];
    let identity = store
        .delivery_identities
        .iter()
        .find(|identity| identity.tool == entry.tool && identity.entry_id == entry.id)
        .map(|identity| identity.artifact_file_identity.as_str())
        .filter(|identity| crate::overlay::creation_file_identity::valid(identity))
        .ok_or_else(|| "Result ownership cannot be verified.".to_string())?;
    if entry.artifact_sha256.len() != 64 {
        return Err("Result ownership cannot be verified.".to_string());
    }
    let file = crate::overlay::creation_delivery::publication::lock_owned_path(
        Path::new(&entry.output_path),
        Some(identity),
        false,
        true,
    )?;
    let artifact = inspect_delivery_artifact(&entry.output_path)?;
    if artifact.file_identity != identity
        || artifact.size_bytes != entry.artifact_size_bytes
        || artifact.sha256 != entry.artifact_sha256
    {
        return Err("Result file changed since it was recorded.".to_string());
    }
    Ok(file)
}

#[cfg(test)]
pub(super) fn delete_all_at(path: &Path, tool: &str) -> Result<usize, CreationError> {
    delete_all_at_protected(path, tool, &protected::ProtectedPaths::default())
}

pub(super) fn delete_all_at_protected(
    path: &Path,
    tool: &str,
    protected: &protected::ProtectedPaths,
) -> Result<usize, CreationError> {
    validate_tool(tool)?;
    let store = load_store(path)?;
    for entry in store.entries.iter().filter(|entry| entry.tool == tool) {
        if protected.conflicts_entry(entry)? {
            return Err(rename::busy("A result is in use by a pending delivery."));
        }
    }
    let ids = store
        .entries
        .into_iter()
        .filter(|entry| entry.tool == tool)
        .map(|entry| entry.id)
        .collect::<Vec<_>>();
    for id in &ids {
        delete_at_protected(path, tool, id, protected)?;
    }
    Ok(ids.len())
}

#[cfg(test)]
mod tests;
