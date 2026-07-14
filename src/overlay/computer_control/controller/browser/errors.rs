use serde_json::Value;

use super::super::{Verb, world::IndexedElement};

#[derive(Debug)]
struct BrowserActionFailure(Value);

impl std::fmt::Display for BrowserActionFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            self.0
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("browser action failed"),
        )
    }
}

impl std::error::Error for BrowserActionFailure {}

pub(super) fn stale_action_failure(
    tab_id: i64,
    selector: &str,
    phase: &str,
    reason: &str,
    document_id: &str,
    element_id: &str,
    observed: Value,
) -> anyhow::Error {
    BrowserActionFailure(serde_json::json!({
        "ok": false,
        "code": "ERR_BROWSER_STALE_TARGET",
        "stale": true,
        "dispatch_ok": false,
        "effect_may_have_occurred": false,
        "error": "the browser document or element changed before input dispatch; observe again",
        "reason": reason,
        "phase": phase,
        "selector": selector,
        "target_tab_id": tab_id,
        "expected": {"document_id": document_id, "element_id": element_id},
        "observed": observed,
    }))
    .into()
}

pub(super) fn result(value: Value) -> anyhow::Result<Value> {
    if value.get("ok").and_then(Value::as_bool) == Some(true) {
        Ok(value)
    } else if value.get("code").and_then(Value::as_str) == Some("ERR_BROWSER_STALE_TARGET") {
        Err(BrowserActionFailure(value).into())
    } else {
        anyhow::bail!(
            "{}",
            value
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("action failed")
        )
    }
}

pub(super) fn typed_value(error: &anyhow::Error) -> Option<Value> {
    error
        .downcast_ref::<BrowserActionFailure>()
        .map(|failure| failure.0.clone())
        .or_else(|| super::super::native::typed_value(error))
}

pub(in crate::overlay::computer_control::controller) fn action_failure(
    error: &anyhow::Error,
    verb: Verb,
    element: &IndexedElement,
    elements: String,
) -> Value {
    if let Some(mut value) = typed_value(error) {
        value["target"] = serde_json::json!({
            "id": element.id,
            "role": element.role,
            "name": element.name,
        });
        value["elements"] = serde_json::json!(elements);
        return value;
    }
    serde_json::json!({
        "ok": false,
        "dispatch_ok": false,
        "effect_may_have_occurred": true,
        "error": format!("could not {} {:?}: {error}", verb.as_str(), element.name),
        "elements": elements,
    })
}

pub(in crate::overlay::computer_control::controller) fn step_failure(
    error: &anyhow::Error,
    step: usize,
    requested_id: u32,
    element: &IndexedElement,
    verb: Verb,
) -> Value {
    let mut value = typed_value(error).unwrap_or_else(|| {
        serde_json::json!({
            "ok": false,
            "effect_may_have_occurred": true,
            "error": error.to_string(),
        })
    });
    value["step"] = serde_json::json!(step);
    value["requested_id"] = serde_json::json!(requested_id);
    value["resolved_id"] = serde_json::json!(element.id);
    value["verb"] = serde_json::json!(verb.as_str());
    value["target"] = serde_json::json!({"role": element.role, "name": element.name});
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_stale_evidence_survives_anyhow_transport() {
        let error = stale_action_failure(
            73,
            "[data-sgt-id=\"4\"]",
            "before_input",
            "document_changed",
            "document-a",
            "element-a",
            serde_json::json!({"documentId":"document-b"}),
        );
        let value = typed_value(&error).expect("typed browser action failure");
        assert_eq!(value["code"], "ERR_BROWSER_STALE_TARGET");
        assert_eq!(value["effect_may_have_occurred"], false);
        assert_eq!(value["target_tab_id"], 73);
    }
}
