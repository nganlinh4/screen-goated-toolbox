use super::*;

pub(super) fn busy(detail: &str) -> CreationError {
    CreationError::new(ErrorCode::ResultBusy, detail)
}

pub(super) fn reconcile_pending_renames(
    store: &mut ResultHistoryStore,
    protected: &protected::ProtectedPaths,
) -> bool {
    if store.pending_renames.is_empty() {
        return false;
    }
    let pending = std::mem::take(&mut store.pending_renames);
    let mut changed = false;
    for rename in pending {
        let valid = validate_tool(&rename.tool).is_ok()
            && rename.previous_path.len() <= MAX_HISTORY_PATH_BYTES
            && rename.next_path.len() <= MAX_HISTORY_PATH_BYTES
            && rename.next_name.len() <= 1_024
            && Path::new(&rename.previous_path).is_absolute()
            && Path::new(&rename.next_path).is_absolute()
            && Path::new(&rename.previous_path).parent() == Path::new(&rename.next_path).parent()
            && rename.artifact_size_bytes > 0
            && rename.artifact_sha256.len() == 64
            && crate::overlay::creation_file_identity::valid(&rename.artifact_file_identity);
        let previous = Path::new(&rename.previous_path);
        let next = Path::new(&rename.next_path);
        if !valid {
            store.pending_renames.push(rename);
            continue;
        }
        let protected_entry = store
            .entries
            .iter()
            .find(|entry| entry.tool == rename.tool && entry.id == rename.entry_id)
            .is_none_or(|entry| protected.conflicts_entry(entry).unwrap_or(true));
        if protected.conflicts_path(previous).unwrap_or(true)
            || protected.conflicts_path(next).unwrap_or(true)
            || protected_entry
        {
            store.pending_renames.push(rename);
            continue;
        }
        let previous_owned = lock_exact_rename_artifact(previous, &rename);
        let next_owned = lock_exact_rename_artifact(next, &rename);
        match (previous_owned, next_owned) {
            (Ok(None), Ok(Some(_))) => {
                if let Some(entry) = store
                    .entries
                    .iter_mut()
                    .find(|entry| entry.tool == rename.tool && entry.id == rename.entry_id)
                {
                    entry.output_path = rename.next_path;
                    entry.output_name = rename.next_name;
                    changed = true;
                } else {
                    store.pending_renames.push(rename);
                }
            }
            (Ok(Some(_)), Ok(None)) => changed = true,
            _ => store.pending_renames.push(rename),
        }
    }
    changed
}

fn lock_exact_rename_artifact(
    path: &Path,
    rename: &PendingRename,
) -> Result<Option<std::fs::File>, ()> {
    match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(()),
        Ok(metadata) if !metadata.file_type().is_file() => return Ok(None),
        Ok(_) => {}
    }
    let file =
        crate::overlay::creation_delivery::publication::lock_owned_path(path, None, false, true)
            .map_err(|_| ())?;
    let artifact =
        crate::overlay::generation_history::inspect_delivery_artifact(&path.to_string_lossy())
            .map_err(|_| ())?;
    Ok((artifact.size_bytes == rename.artifact_size_bytes
        && artifact.sha256 == rename.artifact_sha256
        && artifact.file_identity == rename.artifact_file_identity)
        .then_some(file))
}

fn validated_filename(current: &Path, requested: &str) -> Result<String, String> {
    let requested = requested.trim();
    if requested.is_empty()
        || requested.ends_with(['.', ' '])
        || requested.chars().any(|value| "<>:\"/\\|?*".contains(value))
        || Path::new(requested)
            .file_name()
            .and_then(|name| name.to_str())
            != Some(requested)
    {
        return Err("Enter a valid filename without folders.".to_string());
    }
    let extension = current
        .extension()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "Result extension is missing.".to_string())?;
    let requested_path = Path::new(requested);
    let filename = match requested_path.extension().and_then(|value| value.to_str()) {
        Some(value) if value.eq_ignore_ascii_case(extension) => requested.to_string(),
        Some(_) => return Err(format!("The .{extension} extension cannot be changed.")),
        None => format!("{requested}.{extension}"),
    };
    let stem = Path::new(&filename)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if matches!(
        stem.as_str(),
        "con"
            | "prn"
            | "aux"
            | "nul"
            | "com1"
            | "com2"
            | "com3"
            | "com4"
            | "com5"
            | "com6"
            | "com7"
            | "com8"
            | "com9"
            | "lpt1"
            | "lpt2"
            | "lpt3"
            | "lpt4"
            | "lpt5"
            | "lpt6"
            | "lpt7"
            | "lpt8"
            | "lpt9"
    ) {
        return Err("That filename is reserved by Windows.".to_string());
    }
    Ok(filename)
}

#[cfg(test)]
pub(super) fn rename_at(
    path: &Path,
    tool: &str,
    id: &str,
    new_name: &str,
    results_per_tool: usize,
) -> Result<ResultHistoryEntry, CreationError> {
    rename_at_protected(
        path,
        tool,
        id,
        new_name,
        results_per_tool,
        &protected::ProtectedPaths::default(),
    )
}

pub(super) fn rename_at_protected(
    path: &Path,
    tool: &str,
    id: &str,
    new_name: &str,
    results_per_tool: usize,
    protected: &protected::ProtectedPaths,
) -> Result<ResultHistoryEntry, CreationError> {
    validate_tool(tool)?;
    let mut store = load_store(path)?;
    let protected_paths = protected.expanded(&store)?;
    reconcile_store_protected(path, &mut store, results_per_tool, &protected_paths)?;
    if store
        .pending_renames
        .iter()
        .any(|rename| rename.tool == tool && rename.entry_id == id)
    {
        return Err(busy("Result rename is awaiting recovery."));
    }
    let index = store
        .entries
        .iter()
        .position(|entry| entry.tool == tool && entry.id == id)
        .ok_or_else(|| "Result is no longer in history.".to_string())?;
    let current = PathBuf::from(&store.entries[index].output_path);
    if protected.conflicts_entry(&store.entries[index])? {
        return Err(busy("Result is in use by a pending delivery."));
    }
    if !current.is_file() {
        return Err(CreationError::new(
            ErrorCode::ResultMissing,
            "Result file is no longer on disk.",
        ));
    }
    let filename = validated_filename(&current, new_name)
        .map_err(|error| CreationError::new(ErrorCode::NameInvalid, error))?;
    let target = current
        .parent()
        .ok_or_else(|| "Result folder is missing.".to_string())?
        .join(&filename);
    if protected.conflicts_path(&target)? {
        return Err(busy("Result destination is in use by a pending delivery."));
    }
    if !same_path(&current.to_string_lossy(), &target.to_string_lossy()) && target.exists() {
        return Err(CreationError::new(
            ErrorCode::NameTaken,
            format!("A file named {filename} already exists."),
        ));
    }
    if current == target {
        return Ok(store.entries[index].clone());
    }
    let owned = deletion::lock_recorded_artifact(&store, index)?;
    let artifact = inspect_delivery_artifact(&current.to_string_lossy())?;
    if crate::overlay::creation_file_identity::from_file(&owned)? != artifact.file_identity {
        return Err("Result file changed before it could be renamed.".into());
    }
    // Bind the original pairing before changing the primary's name. Keep the
    // companion locked until the primary rename and history update finish.
    let companion = if companion::checked_from_entry(&store.entries[index])
        .map_err(|_| "Result companion ownership cannot be verified.".to_string())?
        .is_some()
    {
        Some(
            companion::lock_for_entry(&store.entries[index])?
                .ok_or_else(|| "Result companion is no longer on disk.".to_string())?,
        )
    } else {
        None
    };
    if companion.is_some() && store.entries[index].companion_origin_path.is_none() {
        store.entries[index].companion_origin_path = Some(current.to_string_lossy().to_string());
    }
    store.pending_renames.push(PendingRename {
        tool: tool.to_string(),
        entry_id: id.to_string(),
        previous_path: current.to_string_lossy().to_string(),
        next_path: target.to_string_lossy().to_string(),
        next_name: filename.clone(),
        artifact_size_bytes: artifact.size_bytes,
        artifact_sha256: artifact.sha256,
        artifact_file_identity: artifact.file_identity,
    });
    save_store(path, &store)?;
    if let Err(error) = crate::overlay::creation_delivery::publication::rename_owned_no_replace(
        &owned, &current, &target,
    ) {
        store
            .pending_renames
            .retain(|rename| rename.tool != tool || rename.entry_id != id);
        let _ = save_store(path, &store);
        return Err(format!("Could not rename result: {error}").into());
    }
    store.entries[index].output_path = target.to_string_lossy().to_string();
    store.entries[index].output_name = filename;
    store
        .pending_renames
        .retain(|rename| rename.tool != tool || rename.entry_id != id);
    let updated = store.entries[index].clone();
    if let Err(error) = save_store(path, &store) {
        if crate::overlay::creation_delivery::publication::rename_owned_no_replace(
            &owned, &target, &current,
        )
        .is_ok()
        {
            store.entries[index].output_path = current.to_string_lossy().to_string();
            store.entries[index].output_name = current
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default();
            // If this repair write also fails, the previously durable rename
            // record observes the restored original file on restart.
            let _ = save_store(path, &store);
        }
        return Err(error.into());
    }
    drop(companion);
    Ok(updated)
}
