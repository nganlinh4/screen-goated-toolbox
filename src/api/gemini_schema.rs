//! Gemini's supported subset of JSON Schema for constrained generation.

pub(super) fn response_json_schema(schema: &serde_json::Value) -> serde_json::Value {
    let serde_json::Value::Object(object) = schema else {
        return schema.clone();
    };
    let mut supported = serde_json::Map::new();
    for (key, value) in object {
        let sanitized = match key.as_str() {
            "properties" | "$defs" => value.as_object().map(|entries| {
                serde_json::Value::Object(
                    entries
                        .iter()
                        .map(|(name, schema)| (name.clone(), response_json_schema(schema)))
                        .collect(),
                )
            }),
            "items" | "additionalProperties" => Some(response_json_schema(value)),
            "prefixItems" | "anyOf" | "oneOf" => value.as_array().map(|entries| {
                serde_json::Value::Array(entries.iter().map(response_json_schema).collect())
            }),
            "$id" | "$ref" | "$anchor" | "type" | "format" | "title" | "description" | "enum"
            | "minItems" | "maxItems" | "minimum" | "maximum" | "required" | "propertyOrdering" => {
                Some(value.clone())
            }
            _ => None,
        };
        if let Some(value) = sanitized {
            supported.insert(key.clone(), value);
        }
    }
    serde_json::Value::Object(supported)
}

/// A semantically equivalent low-complexity schema for providers that reject
/// otherwise supported constraints. Callers still own semantic validation.
pub(crate) fn compact_response_json_schema(schema: &serde_json::Value) -> serde_json::Value {
    let serde_json::Value::Object(object) = schema else {
        return schema.clone();
    };
    let mut compact = serde_json::Map::new();
    for (key, value) in object {
        let sanitized = match key.as_str() {
            "properties" | "$defs" => value.as_object().map(|entries| {
                serde_json::Value::Object(
                    entries
                        .iter()
                        .map(|(name, schema)| (name.clone(), compact_response_json_schema(schema)))
                        .collect(),
                )
            }),
            "items" => Some(compact_response_json_schema(value)),
            "anyOf" | "oneOf" => value.as_array().map(|entries| {
                serde_json::Value::Array(entries.iter().map(compact_response_json_schema).collect())
            }),
            "$id" | "$ref" | "$anchor" | "type" | "title" | "description" | "enum" | "required"
            | "propertyOrdering" => Some(value.clone()),
            _ => None,
        };
        if let Some(value) = sanitized {
            compact.insert(key.clone(), value);
        }
    }
    serde_json::Value::Object(compact)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_unsupported_keywords_recursively() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "text": {
                    "type": "string",
                    "description": "Detected text",
                    "minLength": 1,
                    "maxLength": 2000,
                    "pattern": "^[a-z]+$"
                }
            },
            "required": ["text"],
            "additionalProperties": false
        });
        let sanitized = response_json_schema(&schema);
        assert_eq!(sanitized["properties"]["text"]["type"], "string");
        assert_eq!(
            sanitized["properties"]["text"]["description"],
            "Detected text"
        );
        assert!(sanitized["properties"]["text"].get("minLength").is_none());
        assert!(sanitized["properties"]["text"].get("maxLength").is_none());
        assert!(sanitized["properties"]["text"].get("pattern").is_none());
        assert_eq!(sanitized["additionalProperties"], false);
    }

    #[test]
    fn compact_schema_keeps_shape_but_removes_constraint_complexity() {
        let schema = serde_json::json!({
            "type": "array",
            "minItems": 1,
            "maxItems": 40,
            "items": {
                "type": "object",
                "properties": { "kind": { "type": "string", "enum": ["a", "b"] } },
                "required": ["kind"],
                "additionalProperties": false
            }
        });
        let compact = compact_response_json_schema(&schema);
        assert_eq!(compact["type"], "array");
        assert_eq!(compact["items"]["required"], serde_json::json!(["kind"]));
        assert_eq!(
            compact["items"]["properties"]["kind"]["enum"],
            serde_json::json!(["a", "b"])
        );
        assert!(compact.get("minItems").is_none());
        assert!(compact["items"].get("additionalProperties").is_none());
    }
}
