//! Privacy-preserving shaping for global Computer Control diagnostics.

use serde_json::{Map, Value, json};

use super::Privacy;

pub(super) fn sanitize_safe_fields(privacy: Privacy, fields: Value) -> Value {
    if privacy != Privacy::Safe {
        return fields;
    }
    sanitize_value(fields, None)
}

fn sanitize_value(value: Value, key: Option<&str>) -> Value {
    match value {
        Value::String(text) if safe_identifier_key(key) => Value::String(text),
        Value::String(text) if safe_sha256_key(key) && valid_sha256(&text) => Value::String(text),
        Value::String(text) => string_metadata(&text),
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .map(|item| sanitize_value(item, key))
                .collect(),
        ),
        Value::Object(fields) => Value::Object(
            fields
                .into_iter()
                .map(|(name, value)| {
                    let value = sanitize_value(value, Some(&name));
                    (name, value)
                })
                .collect(),
        ),
        scalar => scalar,
    }
}

/// Only bounded protocol/status identifiers survive in `Privacy::Safe`
/// records. Unknown string fields are private by default, which keeps future
/// tools from accidentally publishing their arguments or results globally.
fn safe_identifier_key(key: Option<&str>) -> bool {
    matches!(
        key,
        Some(
            "actual_execution_provider"
                | "actual_tool"
                | "browser"
                | "cancel_stage"
                | "capability"
                | "code"
                | "component"
                | "confidence"
                | "delivery_status"
                | "diagnostic_codes"
                | "dispatch_tool"
                | "effect"
                | "effect_status"
                | "endpoint_reason"
                | "effective_tool"
                | "error_code"
                | "event"
                | "finality"
                | "failure_phase"
                | "goal_source"
                | "kind"
                | "model"
                | "name"
                | "outcome"
                | "observation_status"
                | "phase"
                | "privacy"
                | "proof"
                | "provider"
                | "reason"
                | "record_type"
                | "retrieval_status"
                | "requested_execution_provider"
                | "requested_tool"
                | "scope"
                | "source"
                | "source_policy"
                | "stage"
                | "status"
                | "task_class"
                | "tool"
                | "tool_call_id"
                | "tools"
                | "trigger"
                | "turn_mode"
                | "worker"
        )
    )
}

fn safe_sha256_key(key: Option<&str>) -> bool {
    matches!(
        key,
        Some("bundle_sha256" | "input_sha256" | "proof_bundle_sha256" | "vision_bundle_sha256")
    )
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn string_metadata(text: &str) -> Value {
    json!({
        "redacted": true,
        "value_type": "string",
        "char_count": text.chars().count(),
        "byte_count": text.len(),
    })
}

pub(super) fn value_metadata(value: &Value) -> Value {
    match value {
        Value::Null => json!({"value_type": "null"}),
        Value::Bool(_) => json!({"value_type": "boolean"}),
        Value::Number(_) => json!({"value_type": "number"}),
        Value::String(text) => string_metadata(text),
        Value::Array(items) => json!({
            "value_type": "array",
            "item_count": items.len(),
            "item_types": type_counts(items.iter()),
        }),
        Value::Object(fields) => json!({
            "value_type": "object",
            "field_count": fields.len(),
            "value_types": type_counts(fields.values()),
        }),
    }
}

fn type_counts<'a>(values: impl Iterator<Item = &'a Value>) -> Value {
    let mut counts = Map::new();
    for value in values {
        let kind = value_kind(value);
        let count = counts.get(kind).and_then(Value::as_u64).unwrap_or(0) + 1;
        counts.insert(kind.to_string(), json!(count));
    }
    Value::Object(counts)
}

fn value_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_fields_keep_protocol_ids_but_redact_content_by_default() {
        let fields = json!({
            "tool": "run_command",
            "command": "private command",
            "nested": {"url": "https://private.invalid/path", "status": "ok"},
            "worker": "action_executor",
        });

        let sanitized = sanitize_safe_fields(Privacy::Safe, fields);

        assert_eq!(sanitized["tool"], "run_command");
        assert_eq!(sanitized["nested"]["status"], "ok");
        assert_eq!(sanitized["worker"], "action_executor");
        assert_eq!(sanitized["command"]["redacted"], true);
        assert_eq!(sanitized["command"]["char_count"], 15);
        assert_eq!(sanitized["nested"]["url"]["redacted"], true);
        assert!(!sanitized.to_string().contains("private.invalid"));
    }

    #[test]
    fn controlled_diagnostic_code_arrays_remain_actionable() {
        let sanitized = sanitize_safe_fields(
            Privacy::Safe,
            json!({
                "diagnostic_codes": ["source_page_unavailable", "temporary_tab_cleanup_failed"],
                "errors": ["private failure detail"],
            }),
        );

        assert_eq!(sanitized["diagnostic_codes"][0], "source_page_unavailable");
        assert_eq!(
            sanitized["diagnostic_codes"][1],
            "temporary_tab_cleanup_failed"
        );
        assert_eq!(sanitized["errors"][0]["redacted"], true);
    }

    #[test]
    fn structural_outcome_enums_remain_readable() {
        let sanitized = sanitize_safe_fields(
            Privacy::Safe,
            json!({
                "effect_status": "proven_no_effect",
                "failure_phase": "before_activation",
                "observation_status": "unchanged",
                "retrieval_status": "partial",
            }),
        );

        assert_eq!(sanitized["effect_status"], "proven_no_effect");
        assert_eq!(sanitized["failure_phase"], "before_activation");
        assert_eq!(sanitized["observation_status"], "unchanged");
        assert_eq!(sanitized["retrieval_status"], "partial");
    }

    #[test]
    fn safe_fields_keep_named_bundle_hashes_but_not_paths_or_unknown_digests() {
        let fields = json!({
            "proof_bundle_sha256": "a".repeat(64),
            "vision_bundle_sha256": "b".repeat(64),
            "input_sha256": "c".repeat(64),
            "bundle_sha256": "not-a-hash",
            "bundle_artifact_path": "C:/private/proof.json",
            "arbitrary_digest": "d".repeat(64),
        });

        let sanitized = sanitize_safe_fields(Privacy::Safe, fields);

        assert_eq!(sanitized["proof_bundle_sha256"], "a".repeat(64));
        assert_eq!(sanitized["vision_bundle_sha256"], "b".repeat(64));
        assert_eq!(sanitized["input_sha256"], "c".repeat(64));
        assert_eq!(sanitized["bundle_sha256"]["redacted"], true);
        assert_eq!(sanitized["bundle_artifact_path"]["redacted"], true);
        assert_eq!(sanitized["arbitrary_digest"]["redacted"], true);
    }

    #[test]
    fn sensitive_fields_remain_available_to_the_session_trace() {
        let fields = json!({"output": "exact private result"});

        assert_eq!(
            sanitize_safe_fields(Privacy::Sensitive, fields.clone()),
            fields
        );
        assert_eq!(
            sanitize_safe_fields(Privacy::UserText, fields.clone()),
            fields
        );
    }

    #[test]
    fn value_metadata_exposes_shape_not_values_or_field_names() {
        let value = json!({"command": "secret", "count": 7, "enabled": true});
        let metadata = value_metadata(&value);

        assert_eq!(metadata["value_type"], "object");
        assert_eq!(metadata["field_count"], 3);
        assert_eq!(metadata["value_types"]["string"], 1);
        assert!(!metadata.to_string().contains("command"));
        assert!(!metadata.to_string().contains("secret"));
    }
}
