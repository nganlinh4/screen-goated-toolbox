use serde_json::{Value, json};
use std::collections::HashSet;

/// Build the namespaced declared name, ≤64 chars (Gemini's limit), de-duped.
pub(super) fn unique_decl_name(id: &str, tool: &str, seen: &mut HashSet<String>) -> String {
    let mut base: String = format!("mcp__{id}__{tool}").chars().take(64).collect();
    if !seen.contains(&base) {
        seen.insert(base.clone());
        return base;
    }
    let mut n = 2;
    loop {
        let suffix = format!("_{n}");
        let keep = 64usize.saturating_sub(suffix.len());
        base = format!(
            "{}{suffix}",
            format!("mcp__{id}__{tool}")
                .chars()
                .take(keep)
                .collect::<String>()
        );
        if seen.insert(base.clone()) {
            return base;
        }
        n += 1;
    }
}

/// Reduce an MCP JSON-Schema to the OpenAPI subset Gemini accepts (drops `$schema`,
/// `$defs`, `additionalProperties`, `format`, `title`, …); recurses into props/items.
pub(super) fn sanitize_schema(schema: &Value) -> Value {
    let Value::Object(map) = schema else {
        return json!({"type": "object", "properties": {}});
    };
    let mut out = serde_json::Map::new();
    for (key, value) in map {
        match key.as_str() {
            "type" | "description" | "enum" | "required" => {
                out.insert(key.clone(), value.clone());
            }
            "properties" => {
                if let Value::Object(properties) = value {
                    let sanitized = properties
                        .iter()
                        .map(|(name, schema)| (name.clone(), sanitize_schema(schema)))
                        .collect();
                    out.insert(key.clone(), Value::Object(sanitized));
                }
            }
            "items" => {
                out.insert(key.clone(), sanitize_schema(value));
            }
            _ => {}
        }
    }
    if out.get("type").and_then(Value::as_str) == Some("object") && !out.contains_key("properties")
    {
        out.insert("properties".to_string(), json!({}));
    }
    if !out.contains_key("type") {
        out.insert("type".to_string(), json!("object"));
        out.entry("properties").or_insert_with(|| json!({}));
    }
    Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declared_names_are_bounded_and_collision_free() {
        let mut seen = HashSet::new();
        let first = unique_decl_name("future", &"capability".repeat(12), &mut seen);
        let second = unique_decl_name("future", &"capability".repeat(12), &mut seen);

        assert_eq!(first.chars().count(), 64);
        assert_eq!(second.chars().count(), 64);
        assert_ne!(first, second);
        assert!(second.ends_with("_2"));
    }

    #[test]
    fn schema_sanitizing_keeps_supported_structure_recursively() {
        let schema = json!({
            "$schema": "ignored",
            "type": "object",
            "title": "ignored",
            "required": ["items"],
            "properties": {
                "items": {
                    "type": "array",
                    "items": {
                        "type": "string",
                        "format": "ignored",
                        "description": "kept"
                    }
                }
            },
            "additionalProperties": false
        });

        assert_eq!(
            sanitize_schema(&schema),
            json!({
                "type": "object",
                "required": ["items"],
                "properties": {
                    "items": {
                        "type": "array",
                        "items": {"type": "string", "description": "kept"}
                    }
                }
            })
        );
    }

    #[test]
    fn unsupported_or_untyped_schemas_fall_back_to_an_object() {
        let fallback = json!({"type": "object", "properties": {}});

        assert_eq!(sanitize_schema(&Value::Null), fallback);
        assert_eq!(sanitize_schema(&json!({"title": "ignored"})), fallback);
    }
}
