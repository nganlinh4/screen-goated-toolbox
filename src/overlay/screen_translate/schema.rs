//! Provider-neutral translation-only response shape.

pub(super) fn response_schema(member_count: usize, max_text_chars: usize) -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "translations": {
                "type": "array",
                "minItems": member_count,
                "maxItems": member_count,
                "items": {
                    "type": "object",
                    "properties": {
                        "slot": {
                            "type": "integer",
                            "minimum": 0,
                            "maximum": member_count - 1
                        },
                        "translation": {
                            "type": "string",
                            "minLength": 1,
                            "maxLength": max_text_chars
                        }
                    },
                    "required": ["slot", "translation"],
                    "additionalProperties": false
                }
            }
        },
        "required": ["translations"],
        "additionalProperties": false
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn schema_requires_the_complete_known_member_set() {
        let schema = super::response_schema(7, 2000);
        assert_eq!(schema["properties"]["translations"]["minItems"], 7);
        assert_eq!(schema["properties"]["translations"]["maxItems"], 7);
    }
}
