use super::*;

pub(crate) fn live_source_paths() -> Result<std::collections::HashSet<String>, String> {
    let _guard = HISTORY_LOCK
        .lock()
        .map_err(|_| "Result history is unavailable.".to_string())?;
    let store = load_store(&history_path())?;
    let mut paths = std::collections::HashSet::new();
    for entry in store.entries {
        paths.extend((!entry.source_path.is_empty()).then_some(entry.source_path));
        if let Some(values) = entry
            .metadata
            .get("sourceImagePaths")
            .and_then(Value::as_array)
        {
            paths.extend(values.iter().filter_map(Value::as_str).map(str::to_string));
        }
    }
    Ok(paths)
}
