// --- PROVIDER-SPECIFIC REFINE HANDLERS ---
// Gemini, Cerebras, OpenRouter, Groq, and Taalas refinement implementations.

mod groq_compound;

use crate::api::cerebras;
use crate::api::client::{UREQ_AGENT, record_groq_json_usage, record_usage_simple};
use crate::api::gemini_generate::stream_gemini_generate;
use crate::api::openai_compat::stream_openai_compat_chat;
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
        parts,
        p_model,
        gemini_api_key,
        streaming_enabled,
        ui_language,
        cancel_token,
        Some("Gemini Refine Error"),
        false,
        None,
        None,
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

// --- CEREBRAS REFINE ---
pub(super) struct RefineCerebrasRequest<'a> {
    pub cerebras_api_key: &'a str,
    pub final_prompt: &'a str,
    pub previous_text: &'a str,
    pub model: &'a str,
    pub streaming_enabled: bool,
    pub ui_language: &'a str,
    pub cancel_token: &'a Option<Arc<AtomicBool>>,
}

pub(super) fn refine_cerebras<F>(
    request: RefineCerebrasRequest<'_>,
    on_chunk: &mut F,
) -> Result<String>
where
    F: FnMut(&str),
{
    let RefineCerebrasRequest {
        cerebras_api_key,
        final_prompt,
        previous_text,
        model,
        streaming_enabled,
        ui_language,
        cancel_token,
    } = request;
    let messages = serde_json::json!([{ "role": "user", "content": final_prompt }]);
    cerebras::stream_chat(
        cerebras::StreamChatRequest {
            api_key: cerebras_api_key,
            model,
            messages,
            streaming: streaming_enabled,
            ui_language,
            cancel_token,
            error_label: "Cerebras Refine Error",
            response_format: None,
            prediction: Some(previous_text),
            request_timeout: None,
        },
        on_chunk,
    )
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

    let messages = serde_json::json!([{ "role": "user", "content": final_prompt }]);

    stream_openai_compat_chat(
        "https://openrouter.ai/api/v1/chat/completions",
        openrouter_api_key,
        p_model,
        messages,
        streaming_enabled,
        false,
        ui_language,
        cancel_token,
        None,
        "OpenRouter Refine Error",
        false,
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

    let payload = serde_json::json!({
        "model": p_model,
        "messages": [{ "role": "user", "content": final_prompt }],
        "stream": streaming_enabled
    });

    let resp = UREQ_AGENT
        .post("https://api.groq.com/openai/v1/chat/completions")
        .header("Authorization", &format!("Bearer {}", groq_api_key))
        .send_json(payload)
        .map_err(|e| anyhow::anyhow!("Groq Refine Error: {}", e))?;

    record_usage_simple(resp.headers(), p_model);

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
