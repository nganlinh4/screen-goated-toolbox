// --- PROVIDER-SPECIFIC REFINE HANDLERS ---
// Gemini, OpenRouter, Groq, and Taalas refinement implementations.

mod groq_compound;

use crate::api::client::{UREQ_RESPONSE_AGENT, record_groq_json_usage, record_usage_simple};
use crate::api::gemini_generate::{GeminiGenerateRequest, stream_gemini_generate};
use crate::api::openai_compat::stream_openai_compat_payload;
use crate::api::types::ChatCompletionResponse;
use anyhow::Result;
use groq_compound::refine_groq_compound;
use std::io::BufReader;
use std::sync::{Arc, atomic::AtomicBool};

// --- GEMINI REFINE ---
pub(super) fn refine_gemini<F>(
    gemini_api_key: &str,
    final_prompt: &str,
    p_model: &str,
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

    let parts = serde_json::json!([{ "text": final_prompt }]);

    stream_gemini_generate(
        GeminiGenerateRequest {
            parts,
            model: p_model,
            api_key: gemini_api_key,
            streaming: streaming_enabled,
            ui_language,
            cancel_token,
            error_label: Some("Gemini Refine Error"),
            map_auth_errors: false,
            request_timeout: None,
            response_schema: None,
            media_resolution: None,
            retry_observer: None,
        },
        on_chunk,
    )
}

// --- TAALAS REFINE ---
pub(super) fn refine_taalas<F>(
    final_prompt: &str,
    _cancel_token: &Option<Arc<AtomicBool>>,
    on_chunk: &mut F,
) -> Result<String>
where
    F: FnMut(&str),
{
    let text = crate::api::taalas::generate(final_prompt)
        .ok_or_else(|| anyhow::anyhow!("Taalas Refine Error: empty or failed response"))?;
    on_chunk(&text);
    Ok(text)
}

// --- OPENROUTER REFINE ---
pub(super) fn refine_openrouter<F>(
    openrouter_api_key: &str,
    final_prompt: &str,
    p_model: &str,
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

    let mut payload = serde_json::json!({
        "model": p_model,
        "messages": [{ "role": "user", "content": final_prompt }],
        "stream": streaming_enabled
    });
    crate::api::apply_ordinary_openrouter_reasoning_policy(&mut payload, p_model);

    stream_openai_compat_payload(
        "https://openrouter.ai/api/v1/chat/completions",
        openrouter_api_key,
        payload,
        streaming_enabled,
        false,
        ui_language,
        cancel_token,
        None,
        "OpenRouter Refine Error",
        false,
        false,
        |headers| crate::api::client::record_usage_headers("openrouter", p_model, headers),
        |_| {},
        on_chunk,
    )
}

/// NVIDIA NIM refine. OpenAI-compatible with the flat `reasoning_effort` field.
pub(super) fn refine_nvidia<F>(
    nvidia_api_key: &str,
    final_prompt: &str,
    p_model: &str,
    streaming_enabled: bool,
    ui_language: &str,
    cancel_token: &Option<Arc<AtomicBool>>,
    on_chunk: &mut F,
) -> Result<String>
where
    F: FnMut(&str),
{
    if nvidia_api_key.trim().is_empty() {
        return Err(anyhow::anyhow!("NO_API_KEY:nvidia"));
    }

    let mut payload = serde_json::json!({
        "model": p_model,
        "messages": [{ "role": "user", "content": final_prompt }],
        "stream": streaming_enabled
    });
    crate::api::apply_ordinary_openai_reasoning_policy(&mut payload, "nvidia", p_model);

    stream_openai_compat_payload(
        crate::api::NVIDIA_CHAT_COMPLETIONS_URL,
        nvidia_api_key,
        payload,
        streaming_enabled,
        false,
        ui_language,
        cancel_token,
        None,
        "NVIDIA Refine Error",
        false,
        false,
        |headers| crate::api::client::record_usage_headers("nvidia", p_model, headers),
        |_| {},
        on_chunk,
    )
}

// --- GROQ REFINE ---
pub(super) fn refine_groq<F>(
    groq_api_key: &str,
    final_prompt: &str,
    p_model: &str,
    streaming_enabled: bool,
    ui_language: &str,
    cancel_token: &Option<Arc<AtomicBool>>,
    on_chunk: &mut F,
) -> Result<String>
where
    F: FnMut(&str),
{
    if groq_api_key.trim().is_empty() {
        return Err(anyhow::anyhow!("NO_API_KEY:groq"));
    }

    let is_compound = p_model.starts_with("groq/compound");

    if is_compound {
        return refine_groq_compound(groq_api_key, final_prompt, p_model, ui_language, on_chunk);
    }

    let mut payload = serde_json::json!({
        "model": p_model,
        "messages": [{ "role": "user", "content": final_prompt }],
        "stream": streaming_enabled
    });
    crate::api::apply_ordinary_openai_reasoning_policy(&mut payload, "groq", p_model);

    let resp = UREQ_RESPONSE_AGENT
        .post("https://api.groq.com/openai/v1/chat/completions")
        .header("Authorization", &format!("Bearer {}", groq_api_key))
        .send_json(payload)
        .map_err(|e| anyhow::anyhow!("Groq Refine transport error: {}", e))?;

    record_usage_simple(resp.headers(), p_model);
    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body = resp.into_body().read_to_string().unwrap_or_default();
        return Err(anyhow::anyhow!("Groq Refine HTTP {status}: {body}"));
    }

    let mut full_content = String::new();

    if streaming_enabled {
        let reader = BufReader::new(resp.into_body().into_reader());
        full_content =
            crate::api::openai_compat::consume_content_stream(reader, cancel_token, on_chunk)?;
    } else {
        let root: serde_json::Value = resp.into_body().read_json()?;
        record_groq_json_usage(p_model, &root);
        let json: ChatCompletionResponse = serde_json::from_value(root)?;
        if let Some(choice) = json.choices.first() {
            full_content = choice.message.content.clone();
            on_chunk(&full_content);
        }
    }

    Ok(full_content)
}
