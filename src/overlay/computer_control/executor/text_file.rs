//! Exact, bounded UTF-8 file reads and transactional text edits.
//!
//! Model-provided text stays data: no shell, template language, or interpolation
//! is involved. Edits are planned completely in memory, written to a synced
//! sibling, atomically installed, and then read back before success is reported.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::Path;
#[path = "text_file_evidence.rs"]
mod evidence;
#[path = "text_file_failures.rs"]
mod failures;
#[path = "text_file_formula_preservation.rs"]
mod formula_preservation;
#[path = "text_file_match_diagnostics.rs"]
mod match_diagnostics;
#[path = "text_file_read.rs"]
mod stable_read;
#[path = "text_file_structure.rs"]
mod structure;
#[path = "text_file_transaction.rs"]
mod transaction;

use failures::{
    ambiguous_commit, concurrent_change, failure, oversized_edit, replacement_failure, stale,
    transaction_failure,
};
use stable_read::ReadFailure;
#[cfg(test)]
use transaction::atomic_replace;
use transaction::{CommitOutcome, EditGuard, write_synced_sibling};

const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;
const DEFAULT_CONTENT_CHARS: usize = 24_000;
const MAX_CONTENT_CHARS: usize = 64_000;
const MAX_REPLACEMENTS: usize = 64;
const UTF8_BOM: &[u8] = b"\xef\xbb\xbf";
const EDIT_HASH_ERROR: &str = "expected_sha256 must be a 64-character hexadecimal SHA-256; call read_text_file on this path and use its returned sha256—never guess or use a placeholder";

pub(super) fn read_text_file(args: &Value) -> Value {
    let Some(path) = required_string(args, "path") else {
        return failure(
            "ERR_TEXT_FILE_BAD_ARGUMENT",
            None,
            "path must be a non-empty string",
            true,
        );
    };
    let path = Path::new(path);
    if !path.is_absolute() {
        return failure(
            "ERR_TEXT_FILE_PATH_NOT_ABSOLUTE",
            Some(path),
            "path must be absolute; no file was read",
            true,
        );
    }
    let bytes = match read_bounded(path) {
        Ok(bytes) => bytes,
        Err(error) => return error.into_value(path),
    };
    let sha256 = sha256_hex(&bytes);
    if let Some(expected) = args.get("expected_sha256").and_then(Value::as_str) {
        if !valid_hash(expected) {
            return failure(
                "ERR_TEXT_FILE_BAD_ARGUMENT",
                Some(path),
                "expected_sha256 must be a 64-character hexadecimal SHA-256",
                true,
            );
        }
        if !hash_matches(expected, &sha256) {
            return stale(path, expected, &sha256);
        }
    }
    let decoded = match decode_utf8(&bytes) {
        Ok(decoded) => decoded,
        Err(error) => return error.into_value(path),
    };
    let max_chars = args
        .get("max_chars")
        .and_then(Value::as_u64)
        .map(|value| value.clamp(1, MAX_CONTENT_CHARS as u64) as usize)
        .unwrap_or(DEFAULT_CONTENT_CHARS);
    let char_count = decoded.text.chars().count();
    let content: String = decoded.text.chars().take(max_chars).collect();
    let truncated = char_count > max_chars;
    json!({
        "ok": true,
        "path": path.to_string_lossy(),
        "content": content,
        "sha256": sha256,
        "byte_count": bytes.len(),
        "char_count": char_count,
        "line_count": decoded.text.lines().count(),
        "encoding": decoded.encoding,
        "truncated": truncated,
        "content_char_count": char_count.min(max_chars),
        "read_limit_bytes": MAX_FILE_BYTES,
        "original_unchanged": true,
        "completion_proof": {
            "partial": if truncated { vec!["/content"] } else { Vec::<&str>::new() },
            "exact": ["/path", "/sha256"],
        },
    })
}

pub(super) fn edit_text_file(args: &Value) -> Value {
    edit_text_file_with_scope(args, EditScope::Content)
}

pub(super) fn edit_text_file_structure(args: &Value) -> Value {
    edit_text_file_with_scope(args, EditScope::StructurePreflight)
}

pub(in crate::overlay::computer_control) fn commit_text_file_structure(args: &Value) -> Value {
    edit_text_file_with_scope(args, EditScope::StructureCommit)
}

#[derive(Clone, Copy)]
enum EditScope {
    Content,
    StructurePreflight,
    StructureCommit,
}

fn edit_text_file_with_scope(args: &Value, scope: EditScope) -> Value {
    let Some(path_text) = required_string(args, "path") else {
        return failure(
            "ERR_TEXT_FILE_BAD_ARGUMENT",
            None,
            "path must be a non-empty string",
            true,
        );
    };
    let requested_path = Path::new(path_text);
    if !requested_path.is_absolute() {
        return failure(
            "ERR_TEXT_FILE_PATH_NOT_ABSOLUTE",
            Some(requested_path),
            "path must be absolute; no file or directory was changed",
            true,
        );
    }
    let Some(expected_hash) = required_string(args, "expected_sha256") else {
        return failure(
            "ERR_TEXT_FILE_BAD_ARGUMENT",
            Some(requested_path),
            EDIT_HASH_ERROR,
            true,
        );
    };
    if !valid_hash(expected_hash) {
        return failure(
            "ERR_TEXT_FILE_BAD_ARGUMENT",
            Some(requested_path),
            EDIT_HASH_ERROR,
            true,
        );
    }
    let requested_replacements = match parse_replacements(args, requested_path) {
        Ok(replacements) => replacements,
        Err(error) => return error,
    };
    let structural_change_token = match (scope, args.get("structural_change_token")) {
        (EditScope::Content, None) | (EditScope::StructurePreflight, None) => None,
        (EditScope::Content, Some(_)) => {
            return failure(
                "ERR_TEXT_FILE_BAD_ARGUMENT",
                Some(requested_path),
                "edit_text_file never accepts structural authorization; use edit_text_file_structure only when the user explicitly requested a row, column, or formula change",
                true,
            );
        }
        (
            EditScope::StructurePreflight | EditScope::StructureCommit,
            Some(Value::String(value)),
        ) if !value.trim().is_empty() => Some(value.as_str()),
        (EditScope::StructurePreflight | EditScope::StructureCommit, Some(_)) => {
            return failure(
                "ERR_TEXT_FILE_BAD_ARGUMENT",
                Some(requested_path),
                "structural_change_token must be a non-empty string",
                true,
            );
        }
        (EditScope::StructureCommit, None) => {
            return failure(
                "ERR_TEXT_FILE_BAD_ARGUMENT",
                Some(requested_path),
                "structural_change_token must be supplied by the identical non-mutating preflight",
                true,
            );
        }
    };
    let path = match std::fs::canonicalize(requested_path) {
        Ok(path) => path,
        Err(error) => {
            return ReadFailure::from_io(error).into_value(requested_path);
        }
    };
    let mut guard = match EditGuard::acquire(&path) {
        Ok(guard) => guard,
        Err(error) => return transaction_failure(&path, error),
    };
    let original = match guard.read_bounded(MAX_FILE_BYTES) {
        Ok(bytes) => bytes,
        Err(error) => return transaction_failure(&path, error),
    };
    let original_hash = sha256_hex(&original);
    if !hash_matches(expected_hash, &original_hash) {
        return stale(&path, expected_hash, &original_hash);
    }
    let decoded = match decode_utf8(&original) {
        Ok(decoded) => decoded,
        Err(error) => return error.into_value(&path),
    };
    let normalized = match scope {
        EditScope::Content => {
            formula_preservation::normalize(&path, decoded.text, requested_replacements)
        }
        EditScope::StructurePreflight | EditScope::StructureCommit => {
            formula_preservation::without_preservation(requested_replacements)
        }
    };
    let replacements = normalized.replacements;
    let applied = match apply_exact_replacements(decoded.text, &replacements, &path) {
        Ok(applied) => applied,
        Err(error) => return error,
    };
    let edited_text = &applied.text;
    let structure = match scope {
        EditScope::Content => {
            structure::validate_preserving_change(&path, decoded.text, edited_text)
        }
        EditScope::StructurePreflight => {
            structure::validate_explicit_change(&path, decoded.text, edited_text, None)
        }
        EditScope::StructureCommit => structure::validate_explicit_change(
            &path,
            decoded.text,
            edited_text,
            structural_change_token,
        ),
    };
    let structure = match structure {
        Ok(audit) => audit,
        Err(error) => return error,
    };
    let mut edited = Vec::with_capacity(edited_text.len() + usize::from(decoded.has_bom) * 3);
    if decoded.has_bom {
        edited.extend_from_slice(UTF8_BOM);
    }
    edited.extend_from_slice(edited_text.as_bytes());
    if edited.len() as u64 > MAX_FILE_BYTES {
        return oversized_edit(&path, edited.len() as u64);
    }
    if edited == original {
        return failure(
            "ERR_TEXT_FILE_NO_CHANGE",
            Some(&path),
            "the exact replacements would not change the file",
            true,
        );
    }
    let edited_hash = sha256_hex(&edited);
    let content_evidence = evidence::post_edit_content(edited_text, &replacements, &applied.spans);
    let mut partial_proof = Vec::new();
    if content_evidence.truncated {
        partial_proof.push("/content_sample".to_string());
    }
    for index in 0..replacements.len() {
        partial_proof.push(format!("/replacement_evidence/{index}/result_prefix"));
        partial_proof.push(format!("/replacement_evidence/{index}/result_suffix"));
    }
    let temporary = match write_synced_sibling(&path, &edited) {
        Ok(path) => path,
        Err(error) => {
            return failure(
                "ERR_TEXT_FILE_TEMP_WRITE",
                Some(&path),
                &format!("could not stage the replacement: {error}"),
                true,
            );
        }
    };

    // Revalidate through an independent handle immediately before commit. The
    // atomic old-file backup remains the authoritative commit-time race audit.
    let current = match guard.validate_current(&path, &original, MAX_FILE_BYTES) {
        Ok(current) => current,
        Err(change) => {
            return concurrent_change(&path, expected_hash, change.actual_hash.as_deref());
        }
    };
    let retained_backup = match guard.commit_audited(
        &path,
        current,
        temporary,
        &original,
        &edited,
        MAX_FILE_BYTES,
    ) {
        CommitOutcome::Verified { retained_backup } => retained_backup,
        CommitOutcome::NoEffect { error } => {
            return failure("ERR_TEXT_FILE_ATOMIC_REPLACE", Some(&path), &error, true);
        }
        CommitOutcome::Ambiguous {
            error,
            tool_mutated_file,
            external_change_detected,
            recovery_backup,
            recovery_sha256,
        } => {
            return ambiguous_commit(
                &path,
                &error,
                tool_mutated_file,
                external_change_detected,
                recovery_backup.as_deref(),
                recovery_sha256.as_deref(),
            );
        }
    };

    json!({
        "ok": true,
        "path": path.to_string_lossy(),
        "before_sha256": original_hash,
        "sha256": edited_hash,
        "before_byte_count": original.len(),
        "byte_count": edited.len(),
        "content": content_evidence.exact,
        "content_sample": content_evidence.sample,
        "content_truncated": content_evidence.truncated,
        "replacement_evidence": content_evidence.replacements,
        "char_count": edited_text.chars().count(),
        "replacements_applied": replacements.iter().map(|item| item.expected_count).sum::<usize>(),
        "replacement_groups": replacements.len(),
        "requested_replacement_groups": normalized.requested_groups,
        "formula_cells_auto_preserved": normalized.preserved_cells,
        "formula_replacement_groups_rewritten": normalized.rewritten_groups,
        "formula_only_groups_omitted": normalized.omitted_groups,
        "trailing_empty_fields_omitted": normalized.trailing_empty_fields_omitted,
        "trailing_value_fields_repaired": normalized.trailing_value_fields_repaired,
        "edit_scope": match scope {
            EditScope::Content => "content",
            EditScope::StructurePreflight | EditScope::StructureCommit => "structure",
        },
        "structure": structure,
        "encoding": decoded.encoding,
        "atomic": true,
        "durable_stage": true,
        "effect_verified": true,
        "effect_may_have_occurred": true,
        "executed": true,
        "original_unchanged": false,
        "tool_mutated_file": true,
        "external_change_detected": false,
        "retained_backup": retained_backup.map(|path| path.to_string_lossy().to_string()),
        "completion_proof": {
            "postcondition_only": [
                "/sha256", "/byte_count", "/content", "/content_sample",
                "/replacement_evidence", "/char_count", "/structure"
            ],
            "partial": partial_proof,
            "exact": ["/path", "/before_sha256", "/sha256"],
        },
    })
}

struct Decoded<'a> {
    text: &'a str,
    encoding: &'static str,
    has_bom: bool,
}

pub(super) struct Replacement {
    pub(super) old_text: String,
    pub(super) new_text: String,
    pub(super) expected_count: usize,
}

struct Planned<'a> {
    replacement_index: usize,
    start: usize,
    end: usize,
    new_text: &'a str,
}

struct AppliedText {
    text: String,
    spans: Vec<evidence::AppliedSpan>,
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, ReadFailure> {
    stable_read::read_stable_bounded(path, MAX_FILE_BYTES)
}

fn decode_utf8(bytes: &[u8]) -> Result<Decoded<'_>, ReadFailure> {
    let (body, encoding, has_bom) = if bytes.starts_with(UTF8_BOM) {
        (&bytes[UTF8_BOM.len()..], "utf-8-bom", true)
    } else {
        (bytes, "utf-8", false)
    };
    let text = std::str::from_utf8(body).map_err(|_| ReadFailure::UnsupportedEncoding)?;
    Ok(Decoded {
        text,
        encoding,
        has_bom,
    })
}

fn parse_replacements(args: &Value, path: &Path) -> Result<Vec<Replacement>, Value> {
    let Some(items) = args.get("replacements").and_then(Value::as_array) else {
        return Err(failure(
            "ERR_TEXT_FILE_BAD_ARGUMENT",
            Some(path),
            "replacements must be a non-empty array",
            true,
        ));
    };
    if items.is_empty() || items.len() > MAX_REPLACEMENTS {
        return Err(failure(
            "ERR_TEXT_FILE_BAD_ARGUMENT",
            Some(path),
            &format!("replacements must contain 1 to {MAX_REPLACEMENTS} items"),
            true,
        ));
    }
    items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let old_text = item.get("old_text").and_then(Value::as_str);
            let new_text = item.get("new_text").and_then(Value::as_str);
            let expected_count = item.get("expected_count").and_then(Value::as_u64);
            match (old_text, new_text, expected_count) {
                (Some(old_text), Some(new_text), Some(expected_count))
                    if !old_text.is_empty()
                        && expected_count > 0
                        && expected_count <= usize::MAX as u64 =>
                {
                    Ok(Replacement {
                        old_text: old_text.to_string(),
                        new_text: new_text.to_string(),
                        expected_count: expected_count as usize,
                    })
                }
                _ => Err(failure(
                    "ERR_TEXT_FILE_BAD_ARGUMENT",
                    Some(path),
                    &format!(
                        "replacement {index} needs non-empty old_text, string new_text, and positive integer expected_count"
                    ),
                    true,
                )),
            }
        })
        .collect()
}

fn apply_exact_replacements(
    original: &str,
    replacements: &[Replacement],
    path: &Path,
) -> Result<AppliedText, Value> {
    let mut planned = Vec::new();
    for (index, replacement) in replacements.iter().enumerate() {
        let matches: Vec<usize> = original
            .match_indices(&replacement.old_text)
            .map(|(start, _)| start)
            .collect();
        if matches.is_empty() {
            return Err(match_diagnostics::missing_match_failure(
                path,
                index,
                replacement.expected_count,
                original,
                &replacement.old_text,
            ));
        }
        if matches.len() != replacement.expected_count {
            return Err(replacement_failure(
                "ERR_TEXT_FILE_MATCH_AMBIGUOUS",
                path,
                index,
                replacement.expected_count,
                matches.len(),
            ));
        }
        planned.extend(matches.into_iter().map(|start| Planned {
            replacement_index: index,
            start,
            end: start + replacement.old_text.len(),
            new_text: replacement.new_text.as_str(),
        }));
    }
    planned.sort_by_key(|item| (item.start, item.end));
    if planned.windows(2).any(|pair| pair[1].start < pair[0].end) {
        return Err(failure(
            "ERR_TEXT_FILE_OVERLAPPING_REPLACEMENTS",
            Some(path),
            "replacement match ranges overlap",
            true,
        ));
    }
    let mut output = String::with_capacity(original.len());
    let mut spans = Vec::with_capacity(planned.len());
    let mut cursor = 0;
    for item in planned {
        output.push_str(&original[cursor..item.start]);
        let result_start = output.len();
        output.push_str(item.new_text);
        spans.push(evidence::AppliedSpan {
            replacement_index: item.replacement_index,
            start: result_start,
            end: output.len(),
        });
        cursor = item.end;
    }
    output.push_str(&original[cursor..]);
    Ok(AppliedText {
        text: output,
        spans,
    })
}

fn required_string<'a>(args: &'a Value, field: &str) -> Option<&'a str> {
    args.get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
}

fn valid_hash(hash: &str) -> bool {
    hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn hash_matches(expected: &str, actual: &str) -> bool {
    valid_hash(expected) && expected.eq_ignore_ascii_case(actual)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
#[path = "text_file_tests.rs"]
mod tests;
