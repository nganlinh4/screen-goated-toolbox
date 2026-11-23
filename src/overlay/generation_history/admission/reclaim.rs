use super::*;

pub(super) fn reclaimable_entries(
    store: &ResultHistoryStore,
    files: &HashMap<String, u64>,
    protected_paths: &HashSet<String>,
) -> Vec<ResultHistoryEntry> {
    let mut newest_by_tool: HashMap<&str, &ResultHistoryEntry> = HashMap::new();
    for entry in &store.entries {
        let replace = newest_by_tool
            .get(entry.tool.as_str())
            .is_none_or(|newest| {
                (entry.created_at_ms, entry.id.as_str())
                    > (newest.created_at_ms, newest.id.as_str())
            });
        if replace {
            newest_by_tool.insert(entry.tool.as_str(), entry);
        }
    }
    let protected_ids = newest_by_tool
        .values()
        .map(|entry| entry.id.as_str())
        .collect::<HashSet<_>>();
    let mut entries = store
        .entries
        .iter()
        .filter(|entry| {
            entry.managed_artifact
                && entry.artifact_size_bytes > 0
                && entry.artifact_sha256.len() == 64
                && !protected_ids.contains(entry.id.as_str())
                && !protected_paths.contains(&path_key(Path::new(&entry.output_path)))
                && files.get(&path_key(Path::new(&entry.output_path)))
                    == Some(&entry.artifact_size_bytes)
                && match companion::checked_from_entry(entry) {
                    Ok(None) => deletion::reclaimable_primary(store, entry),
                    Ok(Some(_)) => retention_eligible(store, entry),
                    Err(()) => false,
                }
        })
        .cloned()
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.created_at_ms);
    entries
}

pub(super) fn requires_reclamation(budget: Budget) -> bool {
    !matches!(
        required_reclaim(Budget {
            reclaimable_bytes: u64::MAX,
            ..budget
        }),
        Ok(0)
    )
}

pub(super) fn candidate_bytes(entry: &ResultHistoryEntry, files: &HashMap<String, u64>) -> u64 {
    let Some(side) = companion::checked_from_entry(entry).ok().flatten() else {
        return entry.artifact_size_bytes;
    };
    if files.get(&path_key(Path::new(&side.path))) != Some(&side.size_bytes) {
        return entry.artifact_size_bytes;
    }
    let primary = crate::overlay::creation_file_identity::from_path(Path::new(&entry.output_path));
    let secondary = crate::overlay::creation_file_identity::from_path(Path::new(&side.path));
    match (primary, secondary) {
        (Ok(primary), Ok(secondary)) if secondary == side.file_identity && primary != secondary => {
            entry.artifact_size_bytes.saturating_add(side.size_bytes)
        }
        _ => entry.artifact_size_bytes,
    }
}

pub(super) fn plan_reclamation(
    store: &ResultHistoryStore,
    files: &HashMap<String, u64>,
    protected_paths: &HashSet<String>,
    mut budget: Budget,
) -> Result<HashSet<String>, String> {
    let candidates = if requires_reclamation(budget) {
        reclaimable_entries(store, files, protected_paths)
            .into_iter()
            .map(|entry| {
                let bytes = candidate_bytes(&entry, files);
                (entry.id, bytes)
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    budget.reclaimable_bytes = candidates
        .iter()
        .map(|(_, bytes)| *bytes)
        .fold(0_u64, u64::saturating_add);
    let required = required_reclaim(budget)?;
    let mut selected = 0_u64;
    let mut ids = HashSet::new();
    if required > 0 {
        for (id, bytes) in candidates {
            selected = selected.saturating_add(bytes);
            ids.insert(id);
            if selected >= required {
                break;
            }
        }
    }
    Ok(ids)
}
