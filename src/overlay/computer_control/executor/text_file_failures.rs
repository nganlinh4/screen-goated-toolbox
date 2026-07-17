//! Typed, non-mutating failures and ambiguous-commit receipts for text files.

use serde_json::{Map, Value, json};
use std::path::Path;

use super::MAX_FILE_BYTES;
use super::transaction::TransactionFailure;

pub(super) fn replacement_failure(
    code: &str,
    path: &Path,
    index: usize,
    expected: usize,
    actual: usize,
) -> Value {
    let mut value = failure(
        code,
        Some(path),
        "an exact replacement count did not match",
        true,
    );
    if let Some(fields) = value.as_object_mut() {
        fields.insert("replacement_index".to_string(), json!(index));
        fields.insert("expected_count".to_string(), json!(expected));
        fields.insert("actual_count".to_string(), json!(actual));
    }
    value
}

pub(super) fn stale(path: &Path, expected: &str, actual: &str) -> Value {
    let mut value = failure(
        "ERR_TEXT_FILE_STALE",
        Some(path),
        "file hash differs from expected_sha256; read it again before editing",
        true,
    );
    if let Some(fields) = value.as_object_mut() {
        fields.insert("expected_sha256".to_string(), json!(expected));
        fields.insert("actual_sha256".to_string(), json!(actual));
        fields.insert("external_change_detected".to_string(), Value::Bool(true));
    }
    value
}

pub(super) fn failure(
    code: &str,
    path: Option<&Path>,
    message: &str,
    original_unchanged: bool,
) -> Value {
    let mut fields = Map::new();
    fields.insert("ok".to_string(), Value::Bool(false));
    fields.insert("code".to_string(), json!(code));
    fields.insert("error".to_string(), json!(message));
    fields.insert(
        "original_unchanged".to_string(),
        Value::Bool(original_unchanged),
    );
    fields.insert("effect_verified".to_string(), Value::Bool(false));
    fields.insert("effect_may_have_occurred".to_string(), Value::Bool(false));
    fields.insert("executed".to_string(), Value::Bool(false));
    fields.insert("tool_mutated_file".to_string(), Value::Bool(false));
    fields.insert("external_change_detected".to_string(), Value::Bool(false));
    if let Some(path) = path {
        fields.insert("path".to_string(), json!(path.to_string_lossy()));
    }
    Value::Object(fields)
}

pub(super) fn transaction_failure(path: &Path, error: TransactionFailure) -> Value {
    let (code, message) = match error {
        TransactionFailure::Busy(message) => ("ERR_TEXT_FILE_BUSY", message),
        TransactionFailure::Missing => (
            "ERR_TEXT_FILE_MISSING",
            "the file no longer exists".to_string(),
        ),
        TransactionFailure::NotFile => (
            "ERR_TEXT_FILE_NOT_FILE",
            "the path is not a file".to_string(),
        ),
        TransactionFailure::Oversize(size) => (
            "ERR_TEXT_FILE_TOO_LARGE",
            format!("file is {size} bytes; limit is {MAX_FILE_BYTES} bytes"),
        ),
        TransactionFailure::Io(message) => ("ERR_TEXT_FILE_IO", message),
    };
    failure(code, Some(path), &message, true)
}

pub(super) fn oversized_edit(path: &Path, proposed_size: u64) -> Value {
    let mut value = failure(
        "ERR_TEXT_FILE_TOO_LARGE",
        Some(path),
        "the edited file would exceed the text edit size limit",
        true,
    );
    if let Some(fields) = value.as_object_mut() {
        fields.insert("proposed_byte_count".to_string(), json!(proposed_size));
        fields.insert("limit_byte_count".to_string(), json!(MAX_FILE_BYTES));
    }
    value
}

pub(super) fn concurrent_change(path: &Path, expected: &str, actual: Option<&str>) -> Value {
    json!({
        "ok": false,
        "code": "ERR_TEXT_FILE_CONCURRENT_CHANGE",
        "error": "the file path or bytes changed after validation; read it again before editing",
        "path": path.to_string_lossy(),
        "expected_sha256": expected,
        "actual_sha256": actual.unwrap_or("unavailable"),
        "effect_verified": false,
        "effect_may_have_occurred": false,
        "executed": false,
        "original_unchanged": false,
        "tool_mutated_file": false,
        "external_change_detected": true,
    })
}

pub(super) fn ambiguous_commit(
    path: &Path,
    error: &str,
    tool_mutated_file: bool,
    external_change_detected: bool,
    recovery_backup: Option<&Path>,
    recovery_sha256: Option<&str>,
) -> Value {
    json!({
        "ok": false,
        "code": "ERR_TEXT_FILE_COMMIT_AMBIGUOUS",
        "error": error,
        "path": path.to_string_lossy(),
        "effect_verified": false,
        "effect_may_have_occurred": true,
        "executed": Value::Null,
        "original_unchanged": false,
        "tool_mutated_file": tool_mutated_file,
        "external_change_detected": external_change_detected,
        "recovery_required": recovery_backup.is_some(),
        "recovery_backup_path": recovery_backup.map(|path| path.to_string_lossy().to_string()),
        "recovery_backup_sha256": recovery_sha256,
        "recovery_note": "A competing file captured by the atomic replacement is preserved at recovery_backup_path. Do not overwrite either copy until the user chooses which bytes to keep.",
    })
}
