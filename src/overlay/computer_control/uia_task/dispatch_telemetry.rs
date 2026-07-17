//! Content-free result-shape telemetry for local tool dispatch.

use serde_json::{Value, json};

pub(super) fn response_size(result: &Value) -> Value {
    let elements = result.get("elements").and_then(Value::as_str).unwrap_or("");
    json!({
        "serialized_bytes": serde_json::to_vec(result).map_or(0, |bytes| bytes.len()),
        "element_chars": elements.chars().count(),
        "element_lines": elements.lines().count(),
        "elements_unchanged": result.get("elements_unchanged").and_then(Value::as_bool),
        "observation_id": result.pointer("/observation/id"),
        "observation_status": result.pointer("/observation/status"),
        "observation_count": result.pointer("/observation/count"),
    })
}

pub(super) fn structural_failure(result: &Value) -> Value {
    let expected_document = result
        .pointer("/expected/document_id")
        .and_then(Value::as_str);
    let expected_element = result
        .pointer("/expected/element_id")
        .and_then(Value::as_str);
    let observed_document = result
        .pointer("/observed/documentId")
        .and_then(Value::as_str);
    let observed_element = result
        .pointer("/observed/elementId")
        .and_then(Value::as_str);
    json!({
        "reason": result.get("reason"),
        "phase": result.get("phase"),
        "dispatch_ok": result.get("dispatch_ok"),
        "effect_may_have_occurred": result.get("effect_may_have_occurred"),
        "expected_document_present": expected_document.is_some(),
        "expected_element_present": expected_element.is_some(),
        "observed_present": result.pointer("/observed/present"),
        "observed_interactable": result.pointer("/observed/interactable"),
        "observed_focused": result.pointer("/observed/focused"),
        "document_matches": expected_document.zip(observed_document).map(|(a, b)| a == b),
        "element_matches": expected_element.zip(observed_element).map(|(a, b)| a == b),
    })
}

pub(super) fn pre_dispatch_failure(error: impl std::fmt::Display) -> Value {
    json!({
        "ok": false,
        "code": "ERR_INPUT_PREFLIGHT_FAILED",
        "error": error.to_string(),
        "dispatch_ok": false,
        "effect_may_have_occurred": false,
        "effect_verified": false,
        "executed": false,
        "retryable": true,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn pre_dispatch_rejection_proves_that_no_input_ran() {
        let result = super::pre_dispatch_failure("stale target");
        assert_eq!(result["dispatch_ok"], false);
        assert_eq!(result["effect_may_have_occurred"], false);
        assert_eq!(result["executed"], false);
    }
}
