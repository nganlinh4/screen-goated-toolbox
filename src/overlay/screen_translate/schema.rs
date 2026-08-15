//! Provider-neutral constrained-output shape for screen translation.

pub(super) fn response_schema(region_count: usize, max_text_chars: usize) -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "regions": {
                "type": "array",
                "minItems": 1,
                "maxItems": region_count,
                "items": {
                    "type": "object",
                    "properties": {
                        "regionId": { "type": "integer" },
                        "candidateId": { "type": "string", "minLength": 3, "maxLength": 24 },
                        "translationRequirement": {
                            "type": "string",
                            "enum": ["translation_required", "already_target", "non_linguistic"]
                        },
                        "translatedText": { "type": "string", "minLength": 1, "maxLength": max_text_chars }
                    },
                    "required": ["regionId", "candidateId", "translationRequirement", "translatedText"],
                    "additionalProperties": false
                }
            }
        },
        "required": ["regions"],
        "additionalProperties": false
    })
}
