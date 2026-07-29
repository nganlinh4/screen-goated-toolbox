use super::*;

pub(super) fn reconcile_pending_renames(store: &mut ResultHistoryStore) -> bool {
    if store.pending_renames.is_empty() {
        return false;
    }
    let pending = std::mem::take(&mut store.pending_renames);
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
        let previous_owned = valid
            .then(|| lock_exact_rename_artifact(previous, &rename))
            .flatten();
        let next_owned = valid
            .then(|| lock_exact_rename_artifact(next, &rename))
            .flatten();
        match (previous_owned.is_some(), next_owned.is_some()) {
            (false, true) => {
                if let Some(entry) = store
                    .entries
                    .iter_mut()
                    .find(|entry| entry.tool == rename.tool && entry.id == rename.entry_id)
                {
                    entry.output_path = rename.next_path;
                    entry.output_name = rename.next_name;
                }
            }
            (true, false) => {}
            _ => store
                .entries
                .retain(|entry| entry.tool != rename.tool || entry.id != rename.entry_id),
        }
    }
    true
}

fn lock_exact_rename_artifact(path: &Path, rename: &PendingRename) -> Option<std::fs::File> {
    let file = crate::overlay::creation_delivery::publication::lock_owned_path(
        path,
        Some(&rename.artifact_file_identity),
        false,
        true,
    )
    .ok()?;
    let artifact =
        crate::overlay::generation_history::inspect_delivery_artifact(&path.to_string_lossy())
            .ok()?;
    (artifact.size_bytes == rename.artifact_size_bytes
        && artifact.sha256 == rename.artifact_sha256
        && artifact.file_identity == rename.artifact_file_identity)
        .then_some(file)
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

pub(super) fn rename_at(
    path: &Path,
    tool: &str,
    id: &str,
    new_name: &str,
    results_per_tool: usize,
) -> Result<ResultHistoryEntry, String> {
    validate_tool(tool)?;
    let mut store = load_store(path)?;
    reconcile_store(path, &mut store, results_per_tool)?;
    let index = store
        .entries
        .iter()
        .position(|entry| entry.tool == tool && entry.id == id)
        .ok_or_else(|| "Result is no longer in history.".to_string())?;
    let current = PathBuf::from(&store.entries[index].output_path);
    if !current.is_file() {
        return Err("Result file is no longer on disk.".to_string());
    }
    let filename = validated_filename(&current, new_name)?;
    let target = current
        .parent()
        .ok_or_else(|| "Result folder is missing.".to_string())?
        .join(&filename);
    if !same_path(&current.to_string_lossy(), &target.to_string_lossy()) && target.exists() {
        return Err(format!("A file named {filename} already exists."));
    }
    if current == target {
        return Ok(store.entries[index].clone());
    }
    let owned = crate::overlay::creation_delivery::publication::lock_owned_path(
        &current, None, false, true,
    )?;
    let artifact = inspect_delivery_artifact(&current.to_string_lossy())?;
    if crate::overlay::creation_file_identity::from_file(&owned)? != artifact.file_identity {
        return Err("Result file changed before it could be renamed.".to_string());
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
        return Err(format!("Could not rename result: {error}"));
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
        return Err(error);
    }
    Ok(updated)
}
