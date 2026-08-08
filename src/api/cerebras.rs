//! Cerebras chat-completions request policy.
//!
//! Keeps provider-native fields in one place so text and vision cannot drift or
//! silently fall through to another OpenAI-compatible provider.

use crate::api::client::{record_cerebras_json_usage, record_usage_cerebras};
use crate::api::openai_compat::stream_openai_compat_payload;
use anyhow::Result;
use serde_json::{Value, json};
use std::sync::{Arc, atomic::AtomicBool};
use std::time::Duration;

pub const ENDPOINT: &str = "https://api.cerebras.ai/v1/chat/completions";
const MAX_COMPLETION_TOKENS: u32 = 8_192;
pub const MAX_TOOL_ROUNDS: usize = 8;

pub struct StreamChatRequest<'a> {
    pub api_key: &'a str,
    pub model: &'a str,
    pub messages: Value,
    pub streaming: bool,
    pub ui_language: &'a str,
    pub cancel_token: &'a Option<Arc<AtomicBool>>,
    pub error_label: &'a str,
    pub response_format: Option<Value>,
    pub prediction: Option<&'a str>,
    pub request_timeout: Option<Duration>,
}

pub fn chat_payload(
    model: &str,
    messages: Value,
    streaming: bool,
    response_format: Option<Value>,
    prediction: Option<&str>,
) -> Value {
    let mut payload = json!({
        "model": model,
        "messages": messages,
        "stream": streaming,
        "max_completion_tokens": MAX_COMPLETION_TOKENS
    });
    crate::api::apply_ordinary_openai_reasoning_policy(&mut payload, "cerebras", model);
    if let Some(format) = response_format {
        payload["response_format"] = format;
    }
    if supports_predicted_outputs(model)
        && let Some(content) = prediction.filter(|value| !value.is_empty())
    {
        payload["prediction"] = json!({ "type": "content", "content": content });
    }
    payload
}

pub fn stream_chat<F>(request: StreamChatRequest<'_>, on_chunk: &mut F) -> Result<String>
where
    F: FnMut(&str),
{
    let StreamChatRequest {
        api_key,
        model,
        messages,
        streaming,
        ui_language,
        cancel_token,
        error_label,
        response_format,
        prediction,
        request_timeout,
    } = request;
    if api_key.trim().is_empty() {
        return Err(anyhow::anyhow!("NO_API_KEY:cerebras"));
    }
    let payload = chat_payload(model, messages, streaming, response_format, prediction);
    let reasoning_fallback = model.contains("gpt-oss") || model.contains("zai-glm");
    stream_openai_compat_payload(
        ENDPOINT,
        api_key,
        payload,
        streaming,
        reasoning_fallback,
        ui_language,
        cancel_token,
        request_timeout,
        error_label,
        true,
        true,
        |headers| record_usage_cerebras(headers, model),
        |root| record_cerebras_json_usage(model, root),
        on_chunk,
    )
}

pub fn supports_predicted_outputs(model: &str) -> bool {
    model == "gpt-oss-120b" || model == "zai-glm-4.7"
}

/// Models for which Cerebras documents constrained JSON-schema decoding.
pub fn supports_structured_outputs(model: &str) -> bool {
    if crate::model_config::vision_request_profile("cerebras", model).structured_output
        == crate::model_config::StructuredOutputPolicy::StrictJsonSchema
    {
        return true;
    }
    matches!(
        model,
        "gpt-oss-120b"
            | "llama-3.1-8b"
            | "qwen-3-235b-a22b-instruct-2507"
            | "qwen-3-32b"
            | "zai-glm-4.6"
            | "zai-glm-4.7"
    )
}

/// Strict schema shape required by Cerebras constrained decoding.
pub fn strict_json_schema(name: &str, mut schema: Value) -> Value {
    normalize_supported_schema(&mut schema);
    require_closed_objects(&mut schema);
    json!({
        "type": "json_schema",
        "json_schema": { "name": name, "strict": true, "schema": schema }
    })
}

fn normalize_supported_schema(schema: &mut Value) {
    match schema {
        Value::Array(values) => values.iter_mut().for_each(normalize_supported_schema),
        Value::Object(object) => {
            object.remove("minItems");
            object.remove("maxItems");
            object.remove("minLength");
            object.remove("maxLength");
            object.remove("minimum");
            object.remove("maximum");
            object.values_mut().for_each(normalize_supported_schema);
        }
        _ => {}
    }
}

fn require_closed_objects(schema: &mut Value) {
    match schema {
        Value::Array(values) => {
            for value in values {
                require_closed_objects(value);
            }
        }
        Value::Object(object) => {
            for value in object.values_mut() {
                require_closed_objects(value);
            }
            if object.get("type").and_then(Value::as_str) == Some("object") {
                object
                    .entry("additionalProperties")
                    .or_insert_with(|| Value::Bool(false));
            }
        }
        _ => {}
    }
}

/// Return a constrained response format only for models that document it.
///
/// Schema-less JSON mode is intentionally unsupported: callers that need
/// structured output must state the contract they will parse.
pub fn structured_response_format(model: &str, name: &str, schema: &Value) -> Option<Value> {
    supports_structured_outputs(model).then(|| strict_json_schema(name, schema.clone()))
}

/// Build a tool-calling request. Tools and `response_format` are deliberately
/// separate entry points because Cerebras rejects using them together.
pub fn tool_chat_payload(model: &str, messages: Value, tools: Value, streaming: bool) -> Value {
    let mut payload = json!({
        "model": model,
        "messages": messages,
        "tools": tools,
        "tool_choice": "auto",
        "parallel_tool_calls": true,
        "stream": streaming,
        "max_completion_tokens": MAX_COMPLETION_TOKENS
    });
    crate::api::apply_ordinary_openai_reasoning_policy(&mut payload, "cerebras", model);
    payload
}

/// Execute the provider-neutral part of a Cerebras tool loop. The caller owns
/// HTTP and the actual tools; this function owns message ordering, parallel call
/// collection, and the hard round bound.
pub fn run_tool_loop<C, E>(
    model: &str,
    initial_messages: Vec<Value>,
    tools: Value,
    mut complete: C,
    mut execute: E,
) -> Result<String>
where
    C: FnMut(Value) -> Result<Value>,
    E: FnMut(&str, Value) -> Result<String>,
{
    let mut messages = initial_messages;
    for _ in 0..MAX_TOOL_ROUNDS {
        let root = complete(tool_chat_payload(
            model,
            Value::Array(messages.clone()),
            tools.clone(),
            false,
        ))?;
        let assistant = root
            .pointer("/choices/0/message")
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Cerebras tool response has no assistant message"))?;
        let calls = assistant
            .get("tool_calls")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if calls.is_empty() {
            return assistant
                .get("content")
                .and_then(Value::as_str)
                .map(str::to_string)
                .ok_or_else(|| anyhow::anyhow!("Cerebras tool response has no final content"));
        }

        messages.push(assistant);
        for call in calls {
            let call_id = call
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow::anyhow!("Cerebras tool call has no id"))?;
            let function = call
                .get("function")
                .ok_or_else(|| anyhow::anyhow!("Cerebras tool call has no function"))?;
            let name = function
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow::anyhow!("Cerebras tool call has no function name"))?;
            let raw_arguments = function
                .get("arguments")
                .and_then(Value::as_str)
                .unwrap_or("{}");
            let arguments = serde_json::from_str(raw_arguments)
                .map_err(|error| anyhow::anyhow!("Invalid arguments for {name}: {error}"))?;
            let result = execute(name, arguments)?;
            messages.push(json!({
                "role": "tool",
                "tool_call_id": call_id,
                "content": result
            }));
        }
    }
    Err(anyhow::anyhow!(
        "Cerebras tool loop exceeded {MAX_TOOL_ROUNDS} rounds"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_sets_realistic_output_ceiling_and_lowest_reasoning_effort() {
        let payload = chat_payload("gpt-oss-120b", json!([]), false, None, None);
        assert_eq!(payload["max_completion_tokens"], 8_192);
        assert_eq!(payload["reasoning_effort"], "low");
        assert!(payload.get("disable_reasoning").is_none());

        let glm = chat_payload("zai-glm-4.7", json!([]), false, None, None);
        let gemma = chat_payload("gemma-4-31b", json!([]), false, None, None);
        assert_eq!(glm["reasoning_effort"], "none");
        assert_eq!(gemma["reasoning_effort"], "none");
    }

    #[test]
    fn prediction_is_limited_to_documented_models() {
        let supported = chat_payload("zai-glm-4.7", json!([]), false, None, Some("old"));
        let unsupported = chat_payload("gemma-4-31b", json!([]), false, None, Some("old"));
        assert_eq!(supported["prediction"]["content"], "old");
        assert!(unsupported.get("prediction").is_none());
    }

    #[test]
    fn current_gemma_vision_receives_a_strict_schema_only_when_requested() {
        assert!(supports_structured_outputs("gemma-4-31b"));
        assert!(supports_structured_outputs("gpt-oss-120b"));
        assert!(supports_structured_outputs("zai-glm-4.7"));

        let schema = json!({
            "type": "object",
            "properties": { "text": { "type": "string" } },
            "required": ["text"],
            "additionalProperties": false
        });
        assert_eq!(
            structured_response_format("gemma-4-31b", "image", &schema)
                .as_ref()
                .and_then(|value| value.pointer("/json_schema/schema")),
            Some(&schema)
        );
        assert!(structured_response_format("future-model", "image", &schema).is_none());
    }

    #[test]
    fn strict_schema_closes_every_object_for_cerebras() {
        let format = strict_json_schema(
            "nested",
            json!({
                "type": "object",
                "properties": {
                    "point": {
                        "type": "object",
                        "properties": {"x": {"type": "integer"}}
                    }
                }
            }),
        );
        assert_eq!(
            format.pointer("/json_schema/schema/additionalProperties"),
            Some(&Value::Bool(false))
        );
        assert_eq!(
            format.pointer("/json_schema/schema/properties/point/additionalProperties"),
            Some(&Value::Bool(false))
        );
    }

    #[test]
    fn strict_schema_removes_unsupported_validation_bounds() {
        let format = strict_json_schema(
            "points",
            json!({
                "type": "object",
                "properties": {
                    "points": {"type": "array", "minItems": 0, "maxItems": 1, "items": {
                        "type": "object",
                        "properties": {
                            "x": {"type": "integer", "minimum": 0, "maximum": 1000},
                            "label": {"type": "string", "minLength": 1, "maxLength": 160}
                        }
                    }}
                },
                "required": ["points"]
            }),
        );
        let points = format
            .pointer("/json_schema/schema/properties/points")
            .unwrap();
        assert!(points.get("minItems").is_none());
        assert!(points.get("maxItems").is_none());
        assert!(points.pointer("/items/properties/x/minimum").is_none());
        assert!(points.pointer("/items/properties/x/maximum").is_none());
        assert!(
            points
                .pointer("/items/properties/label/minLength")
                .is_none()
        );
        assert!(
            points
                .pointer("/items/properties/label/maxLength")
                .is_none()
        );
    }

    #[test]
    fn tool_payload_never_combines_structured_output_or_prediction() {
        let payload = tool_chat_payload("gpt-oss-120b", json!([]), json!([]), false);
        assert_eq!(payload["parallel_tool_calls"], true);
        assert!(payload.get("response_format").is_none());
        assert!(payload.get("prediction").is_none());
        assert_eq!(MAX_TOOL_ROUNDS, 8);
    }

    #[test]
    fn tool_loop_executes_every_parallel_call_before_final_answer() {
        let mut turn = 0;
        let mut executed = Vec::new();
        let answer = run_tool_loop(
            "gpt-oss-120b",
            vec![json!({ "role": "user", "content": "work" })],
            json!([]),
            |_| {
                turn += 1;
                Ok(if turn == 1 {
                    json!({ "choices": [{ "message": {
                        "role": "assistant", "content": null,
                        "tool_calls": [
                            { "id": "a", "function": { "name": "one", "arguments": "{\"x\":1}" } },
                            { "id": "b", "function": { "name": "two", "arguments": "{\"x\":2}" } }
                        ]
                    }}] })
                } else {
                    json!({ "choices": [{ "message": { "role": "assistant", "content": "done" }}] })
                })
            },
            |name, _| {
                executed.push(name.to_string());
                Ok("ok".to_string())
            },
        )
        .expect("tool loop completes");
        assert_eq!(answer, "done");
        assert_eq!(executed, ["one", "two"]);
    }
}
