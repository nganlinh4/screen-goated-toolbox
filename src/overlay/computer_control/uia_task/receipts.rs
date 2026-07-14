//! Compact, tool-agnostic postcondition receipts for grounding and completion
//! verification. Large content fields stay in the normal tool response/artifact;
//! this trail carries only bounded scalar state from the latest actions.

use serde_json::{Map, Value};

const MAX_RECEIPTS: usize = 6;
const MAX_FIELDS: usize = 6;
const MAX_CANDIDATES: usize = 48;
const MAX_DEPTH: usize = 4;
const MAX_ARRAY_ITEMS: usize = 3;
const MAX_VALUE_CHARS: usize = 120;
const MAX_TOOL_CHARS: usize = 64;
const MAX_PATH_COMPONENT_CHARS: usize = 48;
const MAX_RECEIPT_BYTES: usize = 720;

#[derive(Debug)]
struct Candidate {
    priority: u8,
    path: String,
    value: Value,
}

pub(super) fn push_result(trail: &mut Vec<String>, tool: &str, result: &Value) {
    trail.push(compact_result(tool, result));
    if trail.len() > MAX_RECEIPTS {
        trail.drain(..trail.len() - MAX_RECEIPTS);
    }
}

fn compact_result(tool: &str, result: &Value) -> String {
    let mut receipt = Map::new();
    receipt.insert(
        "tool".to_string(),
        Value::String(tool.chars().take(MAX_TOOL_CHARS).collect()),
    );
    receipt.insert(
        "ok".to_string(),
        Value::Bool(result.get("ok").and_then(Value::as_bool).unwrap_or(true)),
    );

    let mut candidates = Vec::new();
    collect_candidates(result, "", 0, &mut candidates);
    candidates.sort_by(|left, right| {
        left.priority
            .cmp(&right.priority)
            .then_with(|| left.path.cmp(&right.path))
    });
    candidates.dedup_by(|left, right| left.path == right.path);

    let mut state = Map::new();
    for candidate in candidates.into_iter().take(MAX_FIELDS) {
        let mut proposed = state.clone();
        proposed.insert(candidate.path, candidate.value);
        let mut preview = receipt.clone();
        preview.insert("state".to_string(), Value::Object(proposed.clone()));
        if Value::Object(preview).to_string().len() <= MAX_RECEIPT_BYTES {
            state = proposed;
        }
    }
    if !state.is_empty() {
        receipt.insert("state".to_string(), Value::Object(state));
    }
    Value::Object(receipt).to_string()
}

fn collect_candidates(value: &Value, path: &str, depth: usize, out: &mut Vec<Candidate>) {
    if depth >= MAX_DEPTH || out.len() >= MAX_CANDIDATES {
        return;
    }
    match value {
        Value::Object(object) => {
            for (key, child) in object {
                if out.len() >= MAX_CANDIDATES {
                    continue;
                }
                let component: String = key.chars().take(MAX_PATH_COMPONENT_CHARS).collect();
                let child_path = if path.is_empty() {
                    component
                } else {
                    format!("{path}.{component}")
                };
                if (child.is_object() || child.is_array()) && !skip_container(key) {
                    collect_candidates(child, &child_path, depth + 1, out);
                } else if !skip_scalar(key)
                    && let Some(priority) = scalar_priority(key, &child_path)
                    && let Some(value) = bounded_scalar(child)
                {
                    out.push(Candidate {
                        priority,
                        path: child_path,
                        value,
                    });
                }
            }
        }
        Value::Array(items) => {
            for (index, child) in items.iter().take(MAX_ARRAY_ITEMS).enumerate() {
                let child_path = format!("{path}[{index}]");
                collect_candidates(child, &child_path, depth + 1, out);
            }
        }
        _ => {}
    }
}

fn skip_container(key: &str) -> bool {
    matches!(key.to_ascii_lowercase().as_str(), "artifact" | "elements")
}

fn skip_scalar(key: &str) -> bool {
    matches!(
        key.to_ascii_lowercase().as_str(),
        "text"
            | "content"
            | "transcript"
            | "instruction"
            | "answer_material"
            | "html"
            | "markdown"
            | "data"
    )
}

fn scalar_priority(key: &str, path: &str) -> Option<u8> {
    let key = key.to_ascii_lowercase();
    let suffix = |needle: &str| key == needle || key.ends_with(&format!("_{needle}"));
    if matches!(key.as_str(), "code" | "error" | "blocked") {
        Some(0)
    } else if suffix("url") || matches!(key.as_str(), "navigated" | "opened_url") {
        Some(1)
    } else if suffix("title") || key == "name" {
        Some(2)
    } else if suffix("path")
        || matches!(
            key.as_str(),
            "destination" | "source" | "uploaded" | "saved" | "written"
        )
    {
        Some(3)
    } else if matches!(
        key.as_str(),
        "effect" | "effect_verified" | "verify" | "outcome" | "did"
    ) {
        Some(4)
    } else if suffix("state")
        || suffix("status")
        || matches!(
            key.as_str(),
            "selected"
                | "submitted"
                | "found"
                | "moved"
                | "created"
                | "deleted"
                | "launched"
                | "foreground_now"
        )
    {
        Some(5)
    } else if suffix("count") || matches!(key.as_str(), "confidence" | "truncated") {
        Some(6)
    } else if (key == "id" && !path.starts_with("artifact")) || key == "target_tab_id" {
        Some(7)
    } else {
        None
    }
}

fn bounded_scalar(value: &Value) -> Option<Value> {
    match value {
        Value::String(text) => Some(Value::String(truncate(text))),
        Value::Bool(_) | Value::Number(_) => Some(value.clone()),
        _ => None,
    }
}

fn truncate(value: &str) -> String {
    let mut chars = value.chars();
    let mut short: String = chars.by_ref().take(MAX_VALUE_CHARS).collect();
    if chars.next().is_some() {
        short.push('…');
    }
    short
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn browser_page_receipt_keeps_location_without_page_body() {
        let result = json!({
            "ok": true,
            "target_tab_id": 41,
            "page": {
                "title": "Current document",
                "url": "https://example.invalid/current",
                "text": "body-marker".repeat(5000),
                "char_count": 55_000,
            },
            "artifact": {"path": "ignored", "text": "artifact-marker"},
            "instruction": "long guidance-marker",
        });

        let receipt = compact_result("browser_read_page", &result);
        assert!(receipt.contains("Current document"));
        assert!(receipt.contains("https://example.invalid/current"));
        assert!(receipt.contains("char_count"));
        assert!(receipt.contains("target_tab_id"));
        assert!(receipt.contains("41"));
        assert!(!receipt.contains("body-marker"));
        assert!(!receipt.contains("artifact-marker"));
        assert!(!receipt.contains("guidance-marker"));
        assert!(receipt.len() < 600);
    }

    #[test]
    fn unknown_tools_keep_generic_effect_state() {
        let receipt = compact_result(
            "future_provider_action",
            &json!({
                "ok": true,
                "effect": "state_changed",
                "data": {"destination_path": "C:\\Temp\\result.bin"},
                "status": "complete",
                "payload": "not copied",
            }),
        );
        assert!(receipt.contains("future_provider_action"));
        assert!(receipt.contains("state_changed"));
        assert!(receipt.contains("destination_path"));
        assert!(receipt.contains("complete"));
        assert!(!receipt.contains("not copied"));
    }

    #[test]
    fn receipt_trail_retains_only_the_latest_results() {
        let mut trail = Vec::new();
        for index in 0..9 {
            push_result(&mut trail, &format!("tool_{index}"), &json!({"ok": true}));
        }
        assert_eq!(trail.len(), MAX_RECEIPTS);
        assert!(!trail.iter().any(|receipt| receipt.contains("tool_2")));
        assert!(trail.iter().any(|receipt| receipt.contains("tool_8")));
    }
}
