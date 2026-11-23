//! Groq `response_format` selection, shared verbatim by the host and the
//! recorder worker so the strict-schema decision has one implementation.

/// Select the strongest schema mode the catalog declares for the model.
pub(crate) fn structured_response_format(
    model: &str,
    name: &str,
    schema: serde_json::Value,
) -> serde_json::Value {
    if crate::model_config::supports_strict_json_schema("groq", model) {
        serde_json::json!({
            "type": "json_schema",
            "json_schema": { "name": name, "strict": true, "schema": schema }
        })
    } else {
        serde_json::json!({ "type": "json_object" })
    }
}

#[cfg(test)]
mod tests {
    use super::structured_response_format;

    /// These endpoints used strict JSON schema through a hand-kept list; the
    /// catalog now declares it, and every other Groq model keeps json_object.
    #[test]
    fn strict_schema_follows_the_catalog_policy() {
        for model in [
            "openai/gpt-oss-120b",
            "openai/gpt-oss-20b",
            "qwen/qwen3.8-27b",
        ] {
            assert!(
                crate::model_config::supports_strict_json_schema("groq", model),
                "{model}"
            );
            let format = structured_response_format(model, "result", serde_json::json!({}));
            assert_eq!(format["type"], "json_schema", "{model}");
            assert_eq!(format["json_schema"]["strict"], true, "{model}");
        }
        for model in ["whisper-large-v3", "future-groq-model"] {
            assert!(
                !crate::model_config::supports_strict_json_schema("groq", model),
                "{model}"
            );
            let format = structured_response_format(model, "result", serde_json::json!({}));
            assert_eq!(
                format,
                serde_json::json!({ "type": "json_object" }),
                "{model}"
            );
        }
    }
}
