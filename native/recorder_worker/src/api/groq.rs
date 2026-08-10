pub(crate) fn structured_response_format(
    model: &str,
    name: &str,
    schema: serde_json::Value,
) -> serde_json::Value {
    let strict = matches!(model, "openai/gpt-oss-120b" | "openai/gpt-oss-20b")
        || crate::model_config::vision_request_profile("groq", model).structured_output
            == crate::model_config::StructuredOutputPolicy::StrictJsonSchema;
    if strict {
        serde_json::json!({
            "type": "json_schema",
            "json_schema": { "name": name, "strict": true, "schema": schema }
        })
    } else {
        serde_json::json!({ "type": "json_object" })
    }
}
