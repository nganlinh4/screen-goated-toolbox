//! Compact action fingerprints and the single postcondition assessment returned
//! after grounding. Argument values are represented by shape, length, and stable
//! hashes so recovery vision never receives commands, text, paths, queries, or
//! URLs merely because they appeared in an earlier tool call.

use serde_json::{Value, json};

const MAX_FINGERPRINT_CHARS: usize = 384;
const MAX_OBJECT_FIELDS: usize = 12;
const MAX_ARRAY_ITEMS: usize = 4;
const MAX_DEPTH: usize = 4;
const MAX_HISTORY: usize = 8;
const MAX_ADVICE_LATCHES: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NoEffectReason {
    RepeatedUnchangedState,
}

impl NoEffectReason {
    fn code(self) -> &'static str {
        match self {
            Self::RepeatedUnchangedState => "repeated_action_unchanged_state",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(in crate::overlay::computer_control) struct GroundPostcondition {
    no_effect: Option<NoEffectReason>,
    repeated: bool,
    request_advice: bool,
}

impl GroundPostcondition {
    pub(super) fn no_effect(reason: NoEffectReason, repeated: bool, request_advice: bool) -> Self {
        Self {
            no_effect: Some(reason),
            repeated,
            request_advice,
        }
    }

    pub(in crate::overlay::computer_control) fn detected_no_effect(self) -> bool {
        self.no_effect.is_some()
    }

    pub(in crate::overlay::computer_control) fn repeated(self) -> bool {
        self.repeated
    }

    pub(in crate::overlay::computer_control) fn request_advice(self) -> bool {
        self.request_advice
    }

    pub(in crate::overlay::computer_control) fn response(
        self,
        execution_ok: Option<bool>,
        mutating: bool,
        effect_verified: bool,
        advice: Option<String>,
    ) -> Value {
        if execution_ok == Some(false) {
            return json!({
                "ok": false,
                "status": "not_run",
                "effect": "unknown",
                "reason": "execution_failed",
            });
        }
        if let Some(reason) = self.no_effect {
            if effect_verified {
                return json!({
                    "ok": true,
                    "status": "confirmed",
                    "effect": "verified_by_receipt",
                    "heuristic_conflict": reason.code(),
                });
            }
            let mut value = json!({
                "ok": false,
                "status": "checked",
                "effect": "none_detected",
                "reason": reason.code(),
                "repeated": self.repeated,
                "next": if self.repeated {
                    "change_approach_or_stop"
                } else {
                    "reobserve_or_change_approach"
                },
            });
            if let Some(advice) = advice.filter(|text| !text.trim().is_empty()) {
                value["advice"] = Value::String(advice);
            }
            return value;
        }
        if !mutating {
            return json!({
                "status": "not_applicable",
                "effect": "observation_or_query",
            });
        }
        json!({
            "ok": null,
            "status": "not_disproven",
            "confirmed": false,
            "effect": "unknown",
        })
    }
}

pub(super) fn record_action(recent: &mut Vec<String>, tool: &str, args: &Value) -> String {
    let signature = action_fingerprint(tool, args);
    recent.push(signature.clone());
    if recent.len() > MAX_HISTORY {
        recent.drain(..recent.len() - MAX_HISTORY);
    }
    signature
}

pub(super) fn is_repeated_unchanged(
    recent: &[String],
    signature: &str,
    state_changed: bool,
    exempt: bool,
) -> bool {
    !exempt
        && !state_changed
        && recent
            .iter()
            .filter(|item| item.as_str() == signature)
            .count()
            >= 3
}

pub(super) fn latch_advice(
    latches: &mut Vec<String>,
    action_signature: &str,
    state_signature: &str,
) -> bool {
    let key = format!(
        "{}|state={:016x}",
        action_signature,
        fnv1a64(state_signature.as_bytes())
    );
    if latches.contains(&key) {
        return false;
    }
    latches.push(key);
    if latches.len() > MAX_ADVICE_LATCHES {
        latches.drain(..latches.len() - MAX_ADVICE_LATCHES);
    }
    true
}

pub(super) fn action_fingerprint(tool: &str, args: &Value) -> String {
    let mut encoded = String::new();
    encode_value(args, None, false, 0, &mut encoded);
    let tool: String = tool.chars().take(64).collect();
    let mut fingerprint = format!("{tool}|{encoded}");
    fingerprint.truncate(fingerprint.floor_char_boundary(MAX_FINGERPRINT_CHARS));
    fingerprint
}

fn encode_value(
    value: &Value,
    key: Option<&str>,
    coordinate_context: bool,
    depth: usize,
    out: &mut String,
) {
    if depth >= MAX_DEPTH {
        out.push_str("...");
        return;
    }
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(value) => out.push_str(if *value { "bool:1" } else { "bool:0" }),
        Value::Number(value) => {
            if coordinate_context || key.is_some_and(is_safe_numeric_key) {
                out.push_str("num:");
                out.push_str(&value.to_string());
            } else {
                out.push_str(&format!(
                    "num#{:016x}",
                    fnv1a64(value.to_string().as_bytes())
                ));
            }
        }
        Value::String(value) => out.push_str(&format!(
            "str(len={},hash={:016x})",
            value.chars().count(),
            fnv1a64(value.as_bytes())
        )),
        Value::Array(values) => {
            out.push_str(&format!("array(len={})[", values.len()));
            let coordinate_context = coordinate_context || key.is_some_and(is_coordinate_key);
            for (index, value) in values.iter().take(MAX_ARRAY_ITEMS).enumerate() {
                if index > 0 {
                    out.push(',');
                }
                encode_value(value, key, coordinate_context, depth + 1, out);
            }
            if values.len() > MAX_ARRAY_ITEMS {
                out.push_str(",...");
            }
            out.push(']');
        }
        Value::Object(values) => {
            out.push('{');
            let mut entries: Vec<_> = values.iter().collect();
            entries.sort_unstable_by(|left, right| left.0.cmp(right.0));
            for (index, (field, value)) in entries.iter().take(MAX_OBJECT_FIELDS).enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(&safe_field_label(field));
                out.push('=');
                encode_value(value, Some(field), is_coordinate_key(field), depth + 1, out);
            }
            if entries.len() > MAX_OBJECT_FIELDS {
                out.push_str(",...");
            }
            out.push('}');
        }
    }
}

fn safe_field_label(field: &str) -> String {
    if is_safe_numeric_key(field) || is_coordinate_key(field) {
        return field.to_ascii_lowercase();
    }
    format!("k#{:016x}", fnv1a64(field.as_bytes()))
}

fn is_safe_numeric_key(field: &str) -> bool {
    let field = field.to_ascii_lowercase();
    field == "id"
        || field.ends_with("_id")
        || matches!(
            field.as_str(),
            "x" | "y" | "cell" | "row" | "column" | "index"
        )
}

fn is_coordinate_key(field: &str) -> bool {
    let field = field.to_ascii_lowercase();
    field.contains("coord")
        || field.contains("point")
        || field.contains("rect")
        || field.ends_with("_px")
        || field.ends_with("_norm")
        || matches!(field.as_str(), "x" | "y" | "cell" | "row" | "column")
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_fingerprint_never_contains_argument_content() {
        let first = json!({
            "id": 17,
            "cell": 23,
            "screen_px": [640, 480],
            "text": "private sentence",
            "query": "unpublished search",
            "url": "https://private.invalid/secret",
            "path": "C:\\Private\\draft.txt",
            "nested": {"command": "do not expose"},
        });
        let signature = action_fingerprint("future_tool", &first);

        for secret in [
            "private sentence",
            "unpublished search",
            "private.invalid",
            "Private",
            "do not expose",
        ] {
            assert!(!signature.contains(secret));
        }
        assert!(signature.starts_with("future_tool|"));
        assert!(signature.contains("id=num:17"));
        assert!(signature.contains("cell=num:23"));
        assert!(signature.contains("screen_px=array(len=2)[num:640,num:480]"));
        assert!(signature.contains("str(len="));
    }

    #[test]
    fn fingerprint_is_stable_and_content_sensitive_without_disclosure() {
        let left = json!({"text": "alpha", "enabled": true});
        let reordered = json!({"enabled": true, "text": "alpha"});
        let different = json!({"text": "bravo", "enabled": true});
        assert_eq!(
            action_fingerprint("type_text", &left),
            action_fingerprint("type_text", &reordered)
        );
        assert_ne!(
            action_fingerprint("type_text", &left),
            action_fingerprint("type_text", &different)
        );
    }

    #[test]
    fn advice_is_once_per_action_and_state_and_bounded() {
        let mut latches = Vec::new();
        assert!(latch_advice(&mut latches, "click|a", "state-a"));
        assert!(!latch_advice(&mut latches, "click|a", "state-a"));
        assert!(latch_advice(&mut latches, "click|a", "state-b"));
        assert!(latch_advice(&mut latches, "click|b", "state-a"));
        for index in 0..20 {
            assert!(latch_advice(
                &mut latches,
                &format!("tool|{index}"),
                "state"
            ));
        }
        assert_eq!(latches.len(), MAX_ADVICE_LATCHES);
    }

    #[test]
    fn postcondition_response_has_one_compact_failure_channel() {
        let value =
            GroundPostcondition::no_effect(NoEffectReason::RepeatedUnchangedState, true, true)
                .response(
                    Some(true),
                    true,
                    false,
                    Some("Use another visible control.".into()),
                );
        assert_eq!(value["status"], "checked");
        assert_eq!(value["effect"], "none_detected");
        assert_eq!(value["repeated"], true);
        assert!(value.get("advice").is_some());
        assert!(value.get("instruction").is_none());
        assert!(value.get("stuck_warning").is_none());
    }

    #[test]
    fn verified_receipt_wins_over_unchanged_state_heuristic() {
        let value =
            GroundPostcondition::no_effect(NoEffectReason::RepeatedUnchangedState, true, false)
                .response(Some(true), true, true, None);
        assert_eq!(value["ok"], true);
        assert_eq!(value["status"], "confirmed");
        assert_eq!(value["effect"], "verified_by_receipt");
    }
}
