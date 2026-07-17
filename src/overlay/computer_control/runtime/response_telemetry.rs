//! Content-free model-visible tool-response sizing.

use serde_json::Value;

pub(super) fn nested_field<'a>(
    response: &'a Value,
    object: &str,
    field: &str,
) -> Option<&'a Value> {
    response
        .get(object)
        .and_then(|value| value.get(field))
        .or_else(|| {
            response
                .get("action_result")
                .and_then(|value| value.get(object))
                .and_then(|value| value.get(field))
        })
}

pub(super) fn element_shape(response: &Value) -> (usize, usize, bool) {
    let inner = response.get("action_result").unwrap_or(response);
    let elements = inner.get("elements").and_then(Value::as_str).unwrap_or("");
    let count = inner
        .pointer("/observation/count")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let unchanged = inner
        .get("elements_unchanged")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    (elements.chars().count(), count, unchanged)
}
