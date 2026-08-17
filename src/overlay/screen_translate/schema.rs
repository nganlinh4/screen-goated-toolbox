//! Provider-neutral translation-only response shape.

pub(super) fn response_schema(member_count: usize, max_text_chars: usize) -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "members": {
                "type": "array",
                "minItems": member_count,
                "maxItems": member_count,
                "items": {
                    "type": "object",
                    "properties": {
                        "memberId": { "type": "integer" },
                        "translation": {
                            "type": "string",
                            "minLength": 1,
                            "maxLength": max_text_chars
                        }
                    },
                    "required": ["memberId", "translation"],
                    "additionalProperties": false
                }
            }
        },
        "required": ["members"],
        "additionalProperties": false
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn schema_requires_the_complete_known_member_set() {
        let schema = super::response_schema(7, 2000);
        assert_eq!(schema["properties"]["members"]["minItems"], 7);
        assert_eq!(schema["properties"]["members"]["maxItems"], 7);
    }
}
