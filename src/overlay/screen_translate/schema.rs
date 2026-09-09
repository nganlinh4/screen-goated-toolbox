//! Provider-neutral translation-only response shape.

pub(super) fn response_schema(member_count: usize, max_text_chars: usize) -> serde_json::Value {
    let properties = (0..member_count)
        .map(|slot| {
            (
                slot.to_string(),
                serde_json::json!({
                    "type": "string", "minLength": 1, "maxLength": max_text_chars
                }),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    serde_json::json!({
        "type": "object",
        "properties": {
            "translations": {
                "type": "object",
                "properties": properties,
                "required": (0..member_count).map(|slot| slot.to_string()).collect::<Vec<_>>(),
                "additionalProperties": false
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
        let translations = &schema["properties"]["translations"];
        assert_eq!(translations["properties"].as_object().unwrap().len(), 7);
        assert_eq!(
            translations["required"],
            serde_json::json!(["0", "1", "2", "3", "4", "5", "6"])
        );
        assert_eq!(translations["additionalProperties"], false);
    }
}
