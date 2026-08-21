use crate::api::gemini_generate::{GeminiGenerateRequest, stream_gemini_generate};
use crate::api::openai_compat::stream_openai_compat_payload;
use anyhow::Result;
use std::sync::{Arc, atomic::AtomicBool};

use super::TranslateTransportOptions;

// --- GEMINI TEXT API ---
pub(super) fn translate_gemini<F>(
    gemini_api_key: &str,
    model: &str,
    prompt: &str,
    response_schema: Option<&serde_json::Value>,
    transport: TranslateTransportOptions<'_>,
    on_chunk: &mut F,
) -> Result<String>
where
    F: FnMut(&str),
{
    if gemini_api_key.trim().is_empty() {
        return Err(anyhow::anyhow!("NO_API_KEY:gemini"));
    }

    let parts = serde_json::json!([{ "text": prompt }]);

    let result = stream_gemini_generate(
        GeminiGenerateRequest {
            parts,
            model,
            api_key: gemini_api_key,
            streaming: transport.streaming_enabled,
            ui_language: transport.ui_language,
            cancel_token: transport.cancel_token,
            error_label: Some("Gemini Text API Error"),
            map_auth_errors: true,
            request_timeout: transport.request_timeout,
            response_schema,
            media_resolution: None,
            retry_observer: None,
        },
        on_chunk,
    );
    let result = match result {
        Err(error)
            if response_schema.is_some()
                && error.to_string().contains("HTTP 400")
                && error.to_string().contains("INVALID_ARGUMENT") =>
        {
            let Some(schema) = response_schema else {
                return Err(error);
            };
            let compact_schema = crate::api::gemini_schema::compact_response_json_schema(schema);
            stream_gemini_generate(
                GeminiGenerateRequest {
                    parts: serde_json::json!([{ "text": prompt }]),
                    model,
                    api_key: gemini_api_key,
                    streaming: transport.streaming_enabled,
                    ui_language: transport.ui_language,
                    cancel_token: transport.cancel_token,
                    error_label: Some("Gemini Text API Error"),
                    map_auth_errors: true,
                    request_timeout: transport.request_timeout,
                    response_schema: Some(&compact_schema),
                    media_resolution: None,
                    retry_observer: None,
                },
                on_chunk,
            )
        }
        other => other,
    };
    result.map_err(|error| {
        let message = error.to_string();
        if response_schema.is_some()
            && message.contains("HTTP 400")
            && message.contains("INVALID_ARGUMENT")
        {
            anyhow::anyhow!("STRUCTURED_OUTPUT_REJECTED:google:{message}")
        } else {
            error
        }
    })
}

// --- TAALAS API ---
pub(super) fn translate_taalas<F>(
    prompt: &str,
    _cancel_token: &Option<Arc<AtomicBool>>,
    on_chunk: &mut F,
) -> Result<String>
where
    F: FnMut(&str),
{
    let text = crate::api::taalas::generate(prompt)
        .ok_or_else(|| anyhow::anyhow!("Taalas API Error: empty or failed response"))?;
    on_chunk(&text);
    Ok(text)
}

// --- NVIDIA NIM API ---
/// OpenAI-compatible, and it takes the flat `reasoning_effort` field rather than
/// OpenRouter's nested shape, so it uses the shared OpenAI policy applier.
pub(super) fn translate_nvidia<F>(
    nvidia_api_key: &str,
    model: &str,
    prompt: &str,
    response_schema: Option<&serde_json::Value>,
    transport: TranslateTransportOptions<'_>,
    on_chunk: &mut F,
) -> Result<String>
where
    F: FnMut(&str),
{
    if nvidia_api_key.trim().is_empty() {
        return Err(anyhow::anyhow!("NO_API_KEY:nvidia"));
    }

    let mut payload = serde_json::json!({
        "model": model,
        "messages": [{ "role": "user", "content": prompt }],
        "stream": transport.streaming_enabled,
        // Without this NVIDIA applies its own default, which is high enough to
        // corrupt long mixed-script input: unset, roughly one reply in six leaked
        // Korean into the Vietnamese or drifted into other languages entirely.
        // Greedy is also what NVIDIA documents for these endpoints with reasoning
        // disabled, which is how this product calls them.
        "temperature": 0
    });
    if let Some(schema) = response_schema {
        payload["response_format"] = serde_json::json!({
            "type": "json_schema",
            "json_schema": {
                "name": "translation_result",
                "strict": true,
                "schema": schema
            }
        });
    }
    crate::api::apply_ordinary_openai_reasoning_policy(&mut payload, "nvidia", model);

    stream_openai_compat_payload(
        crate::api::NVIDIA_CHAT_COMPLETIONS_URL,
        nvidia_api_key,
        payload,
        transport.streaming_enabled,
        false,
        transport.ui_language,
        transport.cancel_token,
        transport.request_timeout,
        "NVIDIA API Error",
        true,
        false,
        |headers| crate::api::client::record_usage_headers("nvidia", model, headers),
        |_| {},
        on_chunk,
    )
}

// --- OPENROUTER API ---
pub(super) fn translate_openrouter<F>(
    openrouter_api_key: &str,
    model: &str,
    prompt: &str,
    response_schema: Option<&serde_json::Value>,
    transport: TranslateTransportOptions<'_>,
    on_chunk: &mut F,
) -> Result<String>
where
    F: FnMut(&str),
{
    if openrouter_api_key.trim().is_empty() {
        return Err(anyhow::anyhow!("NO_API_KEY:openrouter"));
    }

    let mut payload = serde_json::json!({
        "model": model,
        "messages": [{ "role": "user", "content": prompt }],
        "stream": transport.streaming_enabled
    });
    if let Some(schema) = response_schema {
        payload["response_format"] = serde_json::json!({
            "type": "json_schema",
            "json_schema": {
                "name": "translation_result",
                "strict": true,
                "schema": schema
            }
        });
    }
    crate::api::apply_ordinary_openrouter_reasoning_policy(&mut payload, model);

    stream_openai_compat_payload(
        "https://openrouter.ai/api/v1/chat/completions",
        openrouter_api_key,
        payload,
        transport.streaming_enabled,
        false,
        transport.ui_language,
        transport.cancel_token,
        transport.request_timeout,
        "OpenRouter API Error",
        true,
        false,
        |headers| crate::api::client::record_usage_headers("openrouter", model, headers),
        |_| {},
        on_chunk,
    )
}
