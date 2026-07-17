//! Structured-output contracts for vision localization and verification.

// These are handed to providers with schema support. Localization schemas stay
// loose so a target-not-visible response remains representable.
pub(super) fn point_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "x": {"type": "integer"}, "y": {"type": "integer"},
            "what": {"type": "string"}, "error": {"type": "string"}
        }
    })
}

pub(super) fn box_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "box_2d": {"type": "array", "items": {"type": "integer"}},
            "error": {"type": "string"}
        }
    })
}

pub(super) fn points_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "array",
        "items": {
            "type": "object",
            "properties": {
                "x": {"type": "integer"}, "y": {"type": "integer"}, "what": {"type": "string"}
            }
        }
    })
}

pub(super) fn verification_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "matches": {"type": "boolean"},
            "confidence": {"type": "integer"},
            "what": {"type": "string"}
        },
        "required": ["matches", "confidence", "what"]
    })
}
