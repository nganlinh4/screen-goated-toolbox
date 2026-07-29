//! Structured-output contracts for vision localization and verification.

// These are handed to providers with schema support. Localization schemas stay
// loose so a target-not-visible response remains representable.

pub(super) fn box_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "box_2d": {"type": "array", "items": {"type": "integer"}},
            "error": {"type": "string"}
        }
    })
}
