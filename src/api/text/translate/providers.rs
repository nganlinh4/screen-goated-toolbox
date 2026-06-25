use crate::api::client::record_usage_cerebras;
use crate::api::gemini_generate::stream_gemini_generate;
use crate::api::openai_compat::stream_openai_compat_chat;
use anyhow::Result;
use std::sync::{Arc, atomic::AtomicBool};

// --- GEMINI TEXT API ---
pub(super) fn translate_gemini<F>(
    gemini_api_key: &str,
    model: &str,
    prompt: &str,
    streaming_enabled: bool,
    ui_language: &str,
    cancel_token: &Option<Arc<AtomicBool>>,
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
        streaming_enabled,
        ui_language,
        cancel_token,
        Some("Gemini Text API Error"),
        true,
        None,
        on_chunk,
    )
}

// --- CEREBRAS API ---
pub(super) fn translate_cerebras<F>(
    cerebras_api_key: &str,
    model: &str,
    prompt: &str,
    streaming_enabled: bool,
    ui_language: &str,
    cancel_token: &Option<Arc<AtomicBool>>,
    on_chunk: &mut F,
) -> Result<String>
where
    F: FnMut(&str),
{
    if cerebras_api_key.trim().is_empty() {
        return Err(anyhow::anyhow!("NO_API_KEY:cerebras"));
    }

    let is_reasoning_model = model.contains("gpt-oss") || model.contains("zai-glm");
    let messages = serde_json::json!([{ "role": "user", "content": prompt }]);

    stream_openai_compat_chat(
        "https://api.cerebras.ai/v1/chat/completions",
        cerebras_api_key,
        model,
        messages,
        streaming_enabled,
        is_reasoning_model,
        ui_language,
        cancel_token,
        "Cerebras API Error",
        true,
        |headers| record_usage_cerebras(headers, model),
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
    streaming_enabled: bool,
    ui_language: &str,
    cancel_token: &Option<Arc<AtomicBool>>,
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
        streaming_enabled,
        false,
        ui_language,
        cancel_token,
        "OpenRouter API Error",
        true,
        |_| {},
        on_chunk,
    )
}
