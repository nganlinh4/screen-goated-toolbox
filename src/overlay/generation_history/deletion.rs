use super::*;

pub(super) fn delete_at(path: &Path, tool: &str, id: &str) -> Result<(), String> {
    validate_tool(tool)?;
    let mut store = load_store(path)?;
    let index = store
        .entries
        .iter()
        .position(|entry| entry.tool == tool && entry.id == id)
        .ok_or_else(|| "Result is no longer in history.".to_string())?;
    let output = PathBuf::from(&store.entries[index].output_path);
    companion::delete_for_entry(&store.entries[index])?;
    if output.exists() {
        let owned = crate::overlay::creation_delivery::publication::lock_owned_path(
            &output, None, false, true,
        )?;
        crate::overlay::creation_delivery::publication::delete_owned(&owned, &output)
            .map_err(|error| format!("Could not delete {}: {error}", output.display()))?;
    }
    store.entries.remove(index);
    retain_live_delivery_identities(&mut store);
    save_store(path, &store)
}

pub(super) fn delete_all_at(path: &Path, tool: &str) -> Result<usize, String> {
    validate_tool(tool)?;
    let ids = load_store(path)?
        .entries
        .into_iter()
        .filter(|entry| entry.tool == tool)
        .map(|entry| entry.id)
        .collect::<Vec<_>>();
    for id in &ids {
        delete_at(path, tool, id)?;
    }
    Ok(ids.len())
}
