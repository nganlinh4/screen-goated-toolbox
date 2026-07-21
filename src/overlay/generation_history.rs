use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;

const MAX_RESULTS_PER_TOOL: usize = 250;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultHistoryEntry {
    pub id: String,
    pub tool: String,
    pub source_path: String,
    pub output_path: String,
    pub output_name: String,
    pub created_at_ms: u64,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Default, Deserialize, Serialize)]
struct ResultHistoryStore {
    entries: Vec<ResultHistoryEntry>,
}

static HISTORY_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));
static HISTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn history_path() -> PathBuf {
    crate::paths::app_local_data_dir().join("creation-result-history.json")
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

fn validate_tool(tool: &str) -> Result<(), String> {
    if matches!(tool, "3d" | "svg") {
        Ok(())
    } else {
        Err("Unknown result history tool.".to_string())
    }
}

fn load_store(path: &Path) -> ResultHistoryStore {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|contents| serde_json::from_str(&contents).ok())
        .unwrap_or_default()
}

fn save_store(path: &Path, store: &ResultHistoryStore) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("Could not create result history folder: {error}"))?;
    }
    let contents = serde_json::to_vec_pretty(store)
        .map_err(|error| format!("Could not encode result history: {error}"))?;
    std::fs::write(path, contents)
        .map_err(|error| format!("Could not save result history: {error}"))
}

fn same_path(left: &str, right: &str) -> bool {
    left.eq_ignore_ascii_case(right)
}

fn list_at(path: &Path, tool: &str) -> Result<Vec<ResultHistoryEntry>, String> {
    validate_tool(tool)?;
    let mut store = load_store(path);
    let previous_len = store.entries.len();
    store
        .entries
        .retain(|entry| Path::new(&entry.output_path).is_file());
    if store.entries.len() != previous_len {
        save_store(path, &store)?;
    }
    let mut entries = store
        .entries
        .into_iter()
        .filter(|entry| entry.tool == tool)
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| std::cmp::Reverse(entry.created_at_ms));
    Ok(entries)
}

fn record_at(
    path: &Path,
    tool: &str,
    source_path: &str,
    output_path: &str,
    metadata: Value,
) -> Result<ResultHistoryEntry, String> {
    validate_tool(tool)?;
    let output = PathBuf::from(output_path);
    if !output.is_file() {
        return Err(format!("Result file does not exist: {}", output.display()));
    }
    let output_name = output
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .ok_or_else(|| "Result filename is missing.".to_string())?;
    let output_path = output.to_string_lossy().to_string();
    let mut store = load_store(path);
    let timestamp = now_ms();
    let entry = if let Some(existing) = store
        .entries
        .iter_mut()
        .find(|entry| entry.tool == tool && same_path(&entry.output_path, &output_path))
    {
        existing.source_path = source_path.to_string();
        existing.output_path = output_path;
        existing.output_name = output_name;
        existing.created_at_ms = timestamp;
        existing.metadata = metadata;
        existing.clone()
    } else {
        let entry = ResultHistoryEntry {
            id: format!(
                "{tool}_{timestamp}_{}",
                HISTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ),
            tool: tool.to_string(),
            source_path: source_path.to_string(),
            output_path,
            output_name,
            created_at_ms: timestamp,
            metadata,
        };
        store.entries.push(entry.clone());
        entry
    };
    store
        .entries
        .sort_by_key(|item| std::cmp::Reverse(item.created_at_ms));
    let mut seen_for_tool = 0usize;
    store.entries.retain(|item| {
        if item.tool != tool {
            return true;
        }
        seen_for_tool += 1;
        seen_for_tool <= MAX_RESULTS_PER_TOOL
    });
    save_store(path, &store)?;
    Ok(entry)
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

fn rename_at(
    path: &Path,
    tool: &str,
    id: &str,
    new_name: &str,
) -> Result<ResultHistoryEntry, String> {
    validate_tool(tool)?;
    let mut store = load_store(path);
    let entry = store
        .entries
        .iter_mut()
        .find(|entry| entry.tool == tool && entry.id == id)
        .ok_or_else(|| "Result is no longer in history.".to_string())?;
    let current = PathBuf::from(&entry.output_path);
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
    if current != target {
        std::fs::rename(&current, &target)
            .map_err(|error| format!("Could not rename result: {error}"))?;
    }
    entry.output_path = target.to_string_lossy().to_string();
    entry.output_name = filename;
    let updated = entry.clone();
    save_store(path, &store)?;
    Ok(updated)
}

fn delete_at(path: &Path, tool: &str, id: &str) -> Result<(), String> {
    validate_tool(tool)?;
    let mut store = load_store(path);
    let index = store
        .entries
        .iter()
        .position(|entry| entry.tool == tool && entry.id == id)
        .ok_or_else(|| "Result is no longer in history.".to_string())?;
    let output = PathBuf::from(&store.entries[index].output_path);
    if output.exists() {
        std::fs::remove_file(&output)
            .map_err(|error| format!("Could not delete {}: {error}", output.display()))?;
    }
    store.entries.remove(index);
    save_store(path, &store)
}

pub fn list(tool: &str) -> Result<Vec<ResultHistoryEntry>, String> {
    let _guard = HISTORY_LOCK
        .lock()
        .map_err(|_| "Result history is unavailable.".to_string())?;
    list_at(&history_path(), tool)
}

pub fn record(
    tool: &str,
    source_path: &str,
    output_path: &str,
    metadata: Value,
) -> Result<ResultHistoryEntry, String> {
    let _guard = HISTORY_LOCK
        .lock()
        .map_err(|_| "Result history is unavailable.".to_string())?;
    record_at(&history_path(), tool, source_path, output_path, metadata)
}

pub fn rename(tool: &str, id: &str, new_name: &str) -> Result<ResultHistoryEntry, String> {
    let _guard = HISTORY_LOCK
        .lock()
        .map_err(|_| "Result history is unavailable.".to_string())?;
    rename_at(&history_path(), tool, id, new_name)
}

pub fn delete(tool: &str, id: &str) -> Result<(), String> {
    let _guard = HISTORY_LOCK
        .lock()
        .map_err(|_| "Result history is unavailable.".to_string())?;
    delete_at(&history_path(), tool, id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_missing_results_and_renames_and_deletes_real_files() {
        let root = std::env::temp_dir().join(format!(
            "sgt-result-history-{}-{}",
            std::process::id(),
            now_ms()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let store_path = root.join("history.json");
        let output = root.join("model.glb");
        std::fs::write(&output, b"glTF").unwrap();

        let entry = record_at(
            &store_path,
            "3d",
            "source.png",
            output.to_str().unwrap(),
            serde_json::json!({ "isSegmented": true }),
        )
        .unwrap();
        assert_eq!(list_at(&store_path, "3d").unwrap().len(), 1);

        let renamed = rename_at(&store_path, "3d", &entry.id, "hero").unwrap();
        assert!(renamed.output_path.ends_with("hero.glb"));
        assert!(Path::new(&renamed.output_path).is_file());

        delete_at(&store_path, "3d", &entry.id).unwrap();
        assert!(!Path::new(&renamed.output_path).exists());
        assert!(list_at(&store_path, "3d").unwrap().is_empty());

        let missing = root.join("missing.svg");
        std::fs::write(&missing, b"<svg/>").unwrap();
        record_at(
            &store_path,
            "svg",
            "source.png",
            missing.to_str().unwrap(),
            Value::Null,
        )
        .unwrap();
        std::fs::remove_file(&missing).unwrap();
        assert!(list_at(&store_path, "svg").unwrap().is_empty());
        let _ = std::fs::remove_dir_all(root);
    }
}
