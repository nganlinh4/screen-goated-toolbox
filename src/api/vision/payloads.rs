//! Per-provider vision request payloads.
//!
//! Each provider spells the same request differently: Gemini takes parts, Groq
//! and NVIDIA take the flat OpenAI shape with `reasoning_effort`, and OpenRouter
//! takes a nested `reasoning` object. Keeping the three builders together makes
//! those differences visible side by side rather than scattered through the
//! dispatcher.
//!
//! Every builder reads its request shape, output ceiling and structured-output
//! policy from the catalog rather than hard-coding them.

use super::request_policy;

pub(super) fn groq_vision_payload(
    model: &str,
    prompt: &str,
    mime_type: &str,
    b64_image: &str,
    streaming: bool,
    response_schema: Option<&serde_json::Value>,
) -> serde_json::Value {
    let profile = crate::model_config::vision_request_profile("groq", model);
    let content = request_policy::openai_content(profile, prompt, mime_type, b64_image);
    let mut payload = serde_json::json!({
        "model": model,
        "messages": [
            {
                "role": "user",
                "content": content
            }
        ],
        "temperature": 0.1,
        "stream": streaming
    });
    if profile.sampling == crate::model_config::VisionSamplingPolicy::Qwen3GroqNonThinking {
        payload["reasoning_format"] = "hidden".into();
    }
    if let Some(limit) = profile.max_output_tokens {
        payload["max_completion_tokens"] = limit.into();
    }
    crate::api::apply_ordinary_openai_reasoning_policy(&mut payload, "groq", model);
    request_policy::apply_sampling_policy(&mut payload, profile);
    if let Some(schema) = response_schema {
        payload["response_format"] =
            crate::api::groq::structured_response_format(model, "image_result", schema.clone());
    }
    payload
}

/// NVIDIA NIM vision payload. OpenAI-compatible like OpenRouter, but it takes the
/// flat `reasoning_effort` field rather than the nested `reasoning` object.
pub(super) fn nvidia_vision_payload(
    model: &str,
    prompt: &str,
    mime_type: &str,
    b64_image: &str,
    streaming: bool,
    response_schema: Option<&serde_json::Value>,
) -> serde_json::Value {
    let profile = crate::model_config::vision_request_profile("nvidia", model);
    let content = request_policy::openai_content(profile, prompt, mime_type, b64_image);
    let mut payload = serde_json::json!({
        "model": model,
        "messages": [{ "role": "user", "content": content }],
        "stream": streaming,
        // See translate_nvidia: the provider default is unstable, and greedy is
        // what NVIDIA documents for these endpoints with reasoning disabled.
        "temperature": 0
    });
    if let Some(limit) = profile.max_output_tokens {
        payload["max_completion_tokens"] = limit.into();
    }
    if let Some(schema) = response_schema {
        match profile.structured_output {
            crate::model_config::StructuredOutputPolicy::JsonObject => {
                payload["response_format"] = serde_json::json!({ "type": "json_object" });
            }
            crate::model_config::StructuredOutputPolicy::StrictJsonSchema => {
                payload["response_format"] = serde_json::json!({
                    "type": "json_schema",
                    "json_schema": {
                        "name": "image_result",
                        "strict": true,
                        "schema": schema
                    }
                });
            }
            crate::model_config::StructuredOutputPolicy::Unsupported
            | crate::model_config::StructuredOutputPolicy::PromptOnly => {}
        }
    }
    crate::api::apply_ordinary_openai_reasoning_policy(&mut payload, "nvidia", model);
    payload
}

pub(super) fn openrouter_vision_payload(
    model: &str,
    prompt: &str,
    mime_type: &str,
    b64_image: &str,
    streaming: bool,
    response_schema: Option<&serde_json::Value>,
) -> serde_json::Value {
    let profile = crate::model_config::vision_request_profile("openrouter", model);
    let content = request_policy::openai_content(profile, prompt, mime_type, b64_image);
    let mut payload = serde_json::json!({
        "model": model,
        "messages": [{
            "role": "user",
            "content": content
        }],
        "stream": streaming
    });
    if let Some(limit) = profile.max_output_tokens {
        payload["max_completion_tokens"] = limit.into();
    }
    if let Some(schema) = response_schema {
        match profile.structured_output {
            crate::model_config::StructuredOutputPolicy::JsonObject => {
                payload["response_format"] = serde_json::json!({ "type": "json_object" });
            }
            crate::model_config::StructuredOutputPolicy::StrictJsonSchema => {
                payload["response_format"] = serde_json::json!({
                    "type": "json_schema",
                    "json_schema": {
                        "name": "image_result",
                        "strict": true,
                        "schema": schema
                    }
                });
            }
            crate::model_config::StructuredOutputPolicy::Unsupported
            | crate::model_config::StructuredOutputPolicy::PromptOnly => {}
        }
    }
    crate::api::apply_ordinary_openrouter_reasoning_policy(&mut payload, model);
    payload
}
