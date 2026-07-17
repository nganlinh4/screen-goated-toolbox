use crate::api::cerebras;
use crate::api::gemini_generate::stream_gemini_generate;
use crate::api::openai_compat::stream_openai_compat_chat;
use anyhow::Result;
use std::sync::{Arc, atomic::AtomicBool};
use std::time::Duration;

use super::TranslateTransportOptions;

// --- GEMINI TEXT API ---
pub(super) fn translate_gemini<F>(
    gemini_api_key: &str,
    model: &str,
    prompt: &str,
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

    stream_gemini_generate(
        parts,
        model,
        gemini_api_key,
        transport.streaming_enabled,
        transport.ui_language,
        transport.cancel_token,
        Some("Gemini Text API Error"),
        true,
        transport.request_timeout,
        None,
        on_chunk,
    )
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
            response_format: None,
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
    transport: TranslateTransportOptions<'_>,
    on_chunk: &mut F,
) -> Result<String>
where
    F: FnMut(&str),
{
    if openrouter_api_key.trim().is_empty() {
        return Err(anyhow::anyhow!("NO_API_KEY:openrouter"));
    }

    let messages = serde_json::json!([{ "role": "user", "content": prompt }]);

    stream_openai_compat_chat(
        "https://openrouter.ai/api/v1/chat/completions",
        openrouter_api_key,
        model,
        messages,
        transport.streaming_enabled,
        false,
        transport.ui_language,
        transport.cancel_token,
        transport.request_timeout,
        "OpenRouter API Error",
        true,
        |_| {},
        on_chunk,
    )
}
