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
    let path = std::fs::canonicalize(&path).unwrap_or(path);
    let directory = match std::fs::read_dir(&path) {
        Ok(directory) => directory,
        Err(error) => {
            return failure(
                "filesystem",
                "list_directory",
                &format!("cannot read {}: {error}", path.display()),
                observed_at_ms,
            );
        }
    };
    let mut entries = Vec::new();
    let mut total_entries = 0usize;
    let mut excluded_by_kind = 0usize;
    let mut excluded_by_extension = 0usize;
    for entry in directory.filter_map(Result::ok) {
        let Some(item) = entry_item(&entry.path()) else {
            continue;
        };
        total_entries += 1;
        let item_kind = item.get("kind").and_then(Value::as_str).unwrap_or("");
        if (kind == "file" && item_kind != "file")
            || (kind == "directory" && item_kind != "directory")
        {
            excluded_by_kind += 1;
            continue;
        }
        if item_kind == "file"
            && !extensions.is_empty()
            && !entry
                .path()
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| extensions.contains(&value.to_ascii_lowercase()))
        {
            excluded_by_extension += 1;
            continue;
        }
        entries.push(item);
    }
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
    let mut warnings = Vec::new();
    if total > limit {
        warnings.push(format!("showing {limit} of {total} matching entries"));
    }
    if total == 0 {
        warnings.push(format!(
            "no entries matched in this exact directory; {excluded_by_kind} excluded by kind and {excluded_by_extension} files excluded by extension"
        ));
    }
    let mut result = ok(
        "filesystem",
        "list_directory",
        "rust_std_filesystem",
        "high",
        entries,
        warnings,
        observed_at_ms,
    );
    result["resolved_path"] = json!(path.to_string_lossy());
    result["requested_kind"] = json!(kind);
    result["requested_extensions"] = json!(extensions);
    result["total_entry_count"] = json!(total_entries);
    result["matched_count"] = json!(total);
    result["returned_count"] = json!(result["items"].as_array().map_or(0, Vec::len));
    result["excluded_by_kind"] = json!(excluded_by_kind);
    result["excluded_by_extension"] = json!(excluded_by_extension);
    result["content_coverage"] = json!({
        "status": "metadata_only",
        "listing_complete": total <= limit,
        "returned_file_count": result["items"]
            .as_array()
            .map(|items| items.iter().filter(|item| item["kind"] == "file").count())
            .unwrap_or(0),
        "instruction": "For collection-wide content work, read each in-scope file; report unread, excluded, or omitted entries.",
    });
    result
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
    let path = known.or_else(|| {
        if let Some(rest) = value
            .strip_prefix("~/")
            .or_else(|| value.strip_prefix("~\\"))
        {
            return dirs::home_dir().map(|home| home.join(rest));
        }
        let path = PathBuf::from(value);
        path.is_absolute().then_some(path)
    })?;
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
    use std::sync::atomic::{AtomicU64, Ordering};

    struct TestDir(PathBuf);

    impl TestDir {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(1);
            let path = std::env::temp_dir().join(format!(
                "sgt-list-files-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            assert_eq!(self.0.parent(), Some(std::env::temp_dir().as_path()));
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn missing_path_fails_without_mutation() {
        let result = list_directory(&json!({}), 1);
        assert_eq!(result.get("ok").and_then(Value::as_bool), Some(false));
    }

    #[test]
    fn working_directory_relative_path_is_not_a_filesystem_scope() {
        let result = list_directory(&json!({"path": "."}), 1);
        assert_eq!(result["ok"], false);
        assert!(result["items"].as_array().is_some_and(Vec::is_empty));
    }

    #[test]
    fn extension_filter_keeps_directories_when_kind_allows_them() {
        let root = TestDir::new();
        std::fs::create_dir(root.0.join("nested")).unwrap();
        std::fs::write(root.0.join("included.md"), "included").unwrap();
        std::fs::write(root.0.join("excluded.txt"), "excluded").unwrap();

        let result = list_directory(
            &json!({
                "path": root.0,
                "kind": "any",
                "extensions": ["md"],
                "sort_by": "name",
                "order": "ascending"
            }),
            1,
        );

        assert_eq!(result["ok"], true);
        let names = result["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["name"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["included.md", "nested"]);
        assert_eq!(result["excluded_by_kind"], 0);
        assert_eq!(result["excluded_by_extension"], 1);
        assert_eq!(result["content_coverage"]["status"], "metadata_only");
        assert_eq!(result["content_coverage"]["listing_complete"], true);
        assert_eq!(result["content_coverage"]["returned_file_count"], 1);
        assert!(
            result["content_coverage"]["instruction"]
                .as_str()
                .is_some_and(|instruction| instruction.contains("read each in-scope file"))
        );
        assert_eq!(result["matched_count"], 2);
    }

    #[test]
    fn empty_listing_reports_exact_scope_and_exclusions() {
        let root = TestDir::new();
        std::fs::create_dir(root.0.join("nested")).unwrap();
        std::fs::write(root.0.join("note.md"), "note").unwrap();

        let result = list_directory(
            &json!({"path": root.0, "kind": "file", "extensions": ["csv"]}),
            1,
        );

        assert_eq!(result["ok"], true);
        assert_eq!(result["matched_count"], 0);
        assert_eq!(result["total_entry_count"], 2);
        assert_eq!(result["excluded_by_kind"], 1);
        assert_eq!(result["excluded_by_extension"], 1);
        assert!(
            result["resolved_path"]
                .as_str()
                .is_some_and(|path| Path::new(path).is_absolute())
        );
        assert!(
            result["warnings"][0]
                .as_str()
                .is_some_and(|warning| warning.contains("exact directory"))
        );
    }
}
