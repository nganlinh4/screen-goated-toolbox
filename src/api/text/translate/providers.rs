use crate::api::cerebras;
use crate::api::gemini_generate::{GeminiGenerateRequest, stream_gemini_generate};
use crate::api::openai_compat::stream_openai_compat_payload;
use anyhow::Result;
use std::sync::{Arc, atomic::AtomicBool};
use std::time::Duration;

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

// --- CEREBRAS API ---
pub(super) struct TranslateCerebrasRequest<'a> {
    pub cerebras_api_key: &'a str,
    pub model: &'a str,
    pub instruction: &'a str,
    pub text: &'a str,
    pub streaming_enabled: bool,
    pub ui_language: &'a str,
    pub cancel_token: &'a Option<Arc<AtomicBool>>,
    pub request_timeout: Option<Duration>,
    pub response_schema: Option<&'a serde_json::Value>,
}

pub(super) fn translate_cerebras<F>(
    request: TranslateCerebrasRequest<'_>,
    on_chunk: &mut F,
) -> Result<String>
where
    F: FnMut(&str),
{
    let TranslateCerebrasRequest {
        cerebras_api_key,
        model,
        instruction,
        text,
        streaming_enabled,
        ui_language,
        cancel_token,
        request_timeout,
        response_schema,
    } = request;
    // Static instructions precede dynamic input so Cerebras automatic prefix
    // caching can reuse the stable portion across repeated preset runs.
    let messages = serde_json::json!([
        { "role": "system", "content": instruction },
        { "role": "user", "content": text }
    ]);
    cerebras::stream_chat(
        cerebras::StreamChatRequest {
            api_key: cerebras_api_key,
            model,
            messages,
            streaming: streaming_enabled,
            ui_language,
            cancel_token,
            error_label: "Cerebras API Error",
            response_format: response_schema.and_then(|schema| {
                cerebras::structured_response_format(model, "translation_result", schema)
            }),
            prediction: None,
            request_timeout,
        },
        on_chunk,
    )
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
