use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde_json::{Value, json};

use super::{failure, ok};

pub(super) fn list_directory(args: &Value, observed_at_ms: u128) -> Value {
    let Some(path) = args
        .get("path")
        .and_then(Value::as_str)
        .and_then(resolve_path)
    else {
        return failure(
            "filesystem",
            "list_directory",
            "args.path must name an existing directory or standard folder",
            observed_at_ms,
        );
    };
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(50)
        .clamp(1, 200) as usize;
    let kind = args.get("kind").and_then(Value::as_str).unwrap_or("any");
    let extensions: Vec<String> = args
        .get("extensions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(|value| value.trim_start_matches('.').to_ascii_lowercase())
        .collect();
    let mut entries = match std::fs::read_dir(&path) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .filter_map(|entry| entry_item(&entry.path()).map(|item| (entry, item)))
            .filter(|(entry, _)| match kind {
                "file" => entry.file_type().is_ok_and(|value| value.is_file()),
                "directory" => entry.file_type().is_ok_and(|value| value.is_dir()),
                _ => true,
            })
            .filter(|(entry, _)| {
                extensions.is_empty()
                    || entry
                        .path()
                        .extension()
                        .and_then(|value| value.to_str())
                        .is_some_and(|value| extensions.contains(&value.to_ascii_lowercase()))
            })
            .map(|(_, item)| item)
            .collect::<Vec<_>>(),
        Err(error) => {
            return failure(
                "filesystem",
                "list_directory",
                &format!("cannot read {}: {error}", path.display()),
                observed_at_ms,
            );
        }
    };
    let descending = args
        .get("order")
        .and_then(Value::as_str)
        .unwrap_or("descending")
        != "ascending";
    let sort_by = args
        .get("sort_by")
        .and_then(Value::as_str)
        .unwrap_or("modified");
    entries.sort_by(|left, right| {
        let order = match sort_by {
            "name" => text_field(left, "name").cmp(text_field(right, "name")),
            "size" => number_field(left, "size_bytes").cmp(&number_field(right, "size_bytes")),
            "created" => number_field(left, "created_ms").cmp(&number_field(right, "created_ms")),
            _ => number_field(left, "modified_ms").cmp(&number_field(right, "modified_ms")),
        };
        if descending { order.reverse() } else { order }
    });
    let total = entries.len();
    entries.truncate(limit);
    for (index, item) in entries.iter_mut().enumerate() {
        item["rank"] = json!(index + 1);
    }
    ok(
        "filesystem",
        "list_directory",
        "rust_std_filesystem",
        "high",
        entries,
        (total > limit)
            .then(|| format!("showing {limit} of {total} matching entries"))
            .into_iter()
            .collect(),
        observed_at_ms,
    )
}

fn resolve_path(raw: &str) -> Option<PathBuf> {
    let value = raw.trim();
    let known = match value.to_ascii_lowercase().as_str() {
        "home" => dirs::home_dir(),
        "desktop" => dirs::desktop_dir(),
        "documents" => dirs::document_dir(),
        "downloads" => dirs::download_dir(),
        "music" => dirs::audio_dir(),
        "pictures" => dirs::picture_dir(),
        "videos" => dirs::video_dir(),
        _ => None,
    };
    let path = known.unwrap_or_else(|| {
        if let Some(rest) = value
            .strip_prefix("~/")
            .or_else(|| value.strip_prefix("~\\"))
        {
            return dirs::home_dir().unwrap_or_default().join(rest);
        }
        PathBuf::from(value)
    });
    path.is_dir().then_some(path)
}

fn entry_item(path: &Path) -> Option<Value> {
    let metadata = path.metadata().ok()?;
    Some(json!({
        "name": path.file_name()?.to_string_lossy(),
        "path": path.to_string_lossy(),
        "kind": if metadata.is_dir() { "directory" } else { "file" },
        "extension": path.extension().and_then(|value| value.to_str()),
        "size_bytes": metadata.len(),
        "modified_ms": system_time_ms(metadata.modified().ok()),
        "created_ms": system_time_ms(metadata.created().ok()),
    }))
}

fn system_time_ms(value: Option<std::time::SystemTime>) -> Option<u128> {
    value?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis())
}

fn number_field(value: &Value, field: &str) -> u64 {
    value.get(field).and_then(Value::as_u64).unwrap_or(0)
}

fn text_field<'a>(value: &'a Value, field: &str) -> &'a str {
    value.get(field).and_then(Value::as_str).unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_path_fails_without_mutation() {
        let result = list_directory(&json!({}), 1);
        assert_eq!(result.get("ok").and_then(Value::as_bool), Some(false));
    }
}
