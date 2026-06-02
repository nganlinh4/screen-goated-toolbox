// --- PROVIDER-SPECIFIC REFINE HANDLERS ---
// Gemini, Cerebras, OpenRouter, Groq, and Taalas refinement implementations.

mod groq_compound;

use crate::APP;
use crate::api::client::UREQ_AGENT;
use crate::api::types::{ChatCompletionResponse, StreamChunk};
use crate::gui::locale::LocaleText;
use anyhow::Result;
use groq_compound::refine_groq_compound;
use std::io::{BufRead, BufReader};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

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

    let method = if streaming_enabled {
        "streamGenerateContent"
    } else {
        "generateContent"
    };
    let url = if streaming_enabled {
        format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:{}?alt=sse",
            p_model, method
        )
    } else {
        format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:{}",
            p_model, method
        )
    };

    let mut payload = serde_json::json!({
        "contents": [{ "role": "user", "parts": [{ "text": final_prompt }] }]
    });

    if let Some(thinking_config) = crate::api::gemini_thinking_config(p_model) {
        payload["generationConfig"] = serde_json::json!({
            "thinkingConfig": thinking_config
        });
    }

    if crate::model_config::model_supports_search_by_name(p_model) {
        payload["tools"] = serde_json::json!([
            { "url_context": {} },
            { "google_search": {} }
        ]);
    }

    let resp = UREQ_AGENT
        .post(&url)
        .header("x-goog-api-key", gemini_api_key)
        .send_json(payload)
        .map_err(|e| anyhow::anyhow!("Gemini Refine Error: {}", e))?;

    let mut full_content = String::new();

    if streaming_enabled {
        let reader = BufReader::new(resp.into_body().into_reader());
        let mut thinking_shown = false;
        let mut content_started = false;
        let locale = LocaleText::get(ui_language);

        for line in reader.lines() {
            if let Some(ct) = cancel_token
                && ct.load(Ordering::Relaxed)
            {
                return Err(anyhow::anyhow!("Cancelled"));
            }
            let line = line?;
            if let Some(json_str) = line.strip_prefix("data: ") {
                if json_str.trim() == "[DONE]" {
                    break;
                }
                if let Ok(chunk_resp) = serde_json::from_str::<serde_json::Value>(json_str)
                    && let Some(candidates) =
                        chunk_resp.get("candidates").and_then(|c| c.as_array())
                    && let Some(first) = candidates.first()
                    && let Some(parts) = first
                        .get("content")
                        .and_then(|c| c.get("parts"))
                        .and_then(|p| p.as_array())
                {
                    for part in parts {
                        let is_thought = part
                            .get("thought")
                            .and_then(|t| t.as_bool())
                            .unwrap_or(false);

                        if let Some(t) = part.get("text").and_then(|v| v.as_str()) {
                            if is_thought {
                                if !thinking_shown && !content_started {
                                    on_chunk(locale.model_thinking);
                                    thinking_shown = true;
                                }
                            } else if !content_started && thinking_shown {
                                content_started = true;
                                full_content.push_str(t);
                                let wipe_content =
                                    format!("{}{}", crate::api::WIPE_SIGNAL, full_content);
                                on_chunk(&wipe_content);
                            } else {
                                content_started = true;
                                full_content.push_str(t);
                                on_chunk(t);
                            }
                        }
                    }
                }
            }
        }
    } else {
        let json: serde_json::Value = resp.into_body().read_json()?;
        if let Some(candidates) = json.get("candidates").and_then(|c| c.as_array())
            && let Some(first) = candidates.first()
            && let Some(parts) = first
                .get("content")
                .and_then(|c| c.get("parts"))
                .and_then(|p| p.as_array())
        {
            full_content = parts
                .iter()
                .filter(|p| !p.get("thought").and_then(|t| t.as_bool()).unwrap_or(false))
                .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
                .collect::<String>();
            on_chunk(&full_content);
        }
    }

    Ok(full_content)
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
pub(super) fn refine_cerebras<F>(
    cerebras_api_key: &str,
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
    if cerebras_api_key.trim().is_empty() {
        return Err(anyhow::anyhow!("NO_API_KEY:cerebras"));
    }

    let payload = serde_json::json!({
        "model": p_model,
        "messages": [
            { "role": "user", "content": final_prompt }
        ],
        "stream": streaming_enabled
    });

    let resp = UREQ_AGENT
        .post("https://api.cerebras.ai/v1/chat/completions")
        .header("Authorization", &format!("Bearer {}", cerebras_api_key))
        .header("Content-Type", "application/json")
        .send_json(payload)
        .map_err(|e| anyhow::anyhow!("Cerebras Refine Error: {}", e))?;

    let remaining = resp
        .headers()
        .get("x-ratelimit-remaining-requests-day")
        .or_else(|| resp.headers().get("x-ratelimit-remaining-requests"))
        .and_then(|v| v.to_str().ok())
        .unwrap_or("?");

    let mut limit = resp
        .headers()
        .get("x-ratelimit-limit-requests-day")
        .or_else(|| resp.headers().get("x-ratelimit-limit-requests"))
        .and_then(|v| v.to_str().ok())
        .unwrap_or("?")
        .to_string();

    if limit == "?"
        && let Some(conf) = crate::model_config::get_model_by_id(p_model)
        && let Some(val) = conf.quota_limit_en.split_whitespace().next()
    {
        limit = val.to_string();
    }

    if remaining != "?" || limit != "?" {
        let usage_str = format!("{} / {}", remaining, limit);
        if let Ok(mut app) = APP.lock() {
            app.model_usage_stats.insert(p_model.to_string(), usage_str);
        }
    }

    let mut full_content = String::new();

    if streaming_enabled {
        let reader = BufReader::new(resp.into_body().into_reader());
        let mut thinking_shown = false;
        let mut content_started = false;
        let locale = LocaleText::get(ui_language);

        let is_reasoning_model = p_model.contains("gpt-oss") || p_model.contains("zai-glm");

        for line in reader.lines() {
            if let Some(ct) = cancel_token
                && ct.load(Ordering::Relaxed)
            {
                return Err(anyhow::anyhow!("Cancelled"));
            }
            let line = line?;
            if let Some(data) = line.strip_prefix("data: ") {
                if data == "[DONE]" {
                    break;
                }

                match serde_json::from_str::<StreamChunk>(data) {
                    Ok(chunk) => {
                        if let Some(reasoning) = chunk
                            .choices
                            .first()
                            .and_then(|c| c.delta.reasoning.as_ref())
                            .filter(|s| !s.is_empty())
                        {
                            if !thinking_shown && !content_started {
                                on_chunk(locale.model_thinking);
                                thinking_shown = true;
                            }
                            let _ = reasoning;
                        } else if is_reasoning_model && !content_started && !thinking_shown {
                            on_chunk(locale.model_thinking);
                            thinking_shown = true;
                        }

                        if let Some(content) = chunk
                            .choices
                            .first()
                            .and_then(|c| c.delta.content.as_ref())
                            .filter(|s| !s.is_empty())
                        {
                            if !content_started && thinking_shown {
                                content_started = true;
                                full_content.push_str(content);
                                let wipe_content =
                                    format!("{}{}", crate::api::WIPE_SIGNAL, full_content);
                                on_chunk(&wipe_content);
                            } else {
                                content_started = true;
                                full_content.push_str(content);
                                on_chunk(content);
                            }
                        }
                    }
                    Err(_) => continue,
                }
            }
        }
    } else {
        let json: ChatCompletionResponse = resp.into_body().read_json()?;
        if let Some(choice) = json.choices.first() {
            full_content = choice.message.content.clone();
            on_chunk(&full_content);
        }
    }

    Ok(full_content)
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

    let payload = serde_json::json!({
        "model": p_model,
        "messages": [
            { "role": "user", "content": final_prompt }
        ],
        "stream": streaming_enabled
    });

    let resp = UREQ_AGENT
        .post("https://openrouter.ai/api/v1/chat/completions")
        .header("Authorization", &format!("Bearer {}", openrouter_api_key))
        .header("Content-Type", "application/json")
        .send_json(payload)
        .map_err(|e| anyhow::anyhow!("OpenRouter Refine Error: {}", e))?;

    let mut full_content = String::new();

    if streaming_enabled {
        let reader = BufReader::new(resp.into_body().into_reader());
        let mut thinking_shown = false;
        let mut content_started = false;
        let locale = LocaleText::get(ui_language);

        for line in reader.lines() {
            if let Some(ct) = cancel_token
                && ct.load(Ordering::Relaxed)
            {
                return Err(anyhow::anyhow!("Cancelled"));
            }
            let line = line?;
            if let Some(data) = line.strip_prefix("data: ") {
                if data == "[DONE]" {
                    break;
                }

                match serde_json::from_str::<StreamChunk>(data) {
                    Ok(chunk) => {
                        if let Some(reasoning) = chunk
                            .choices
                            .first()
                            .and_then(|c| c.delta.reasoning.as_ref())
                            .filter(|s| !s.is_empty())
                        {
                            if !thinking_shown && !content_started {
                                on_chunk(locale.model_thinking);
                                thinking_shown = true;
                            }
                            let _ = reasoning;
                        }

                        if let Some(content) = chunk
                            .choices
                            .first()
                            .and_then(|c| c.delta.content.as_ref())
                            .filter(|s| !s.is_empty())
                        {
                            if !content_started && thinking_shown {
                                content_started = true;
                                full_content.push_str(content);
                                let wipe_content =
                                    format!("{}{}", crate::api::WIPE_SIGNAL, full_content);
                                on_chunk(&wipe_content);
                            } else {
                                content_started = true;
                                full_content.push_str(content);
                                on_chunk(content);
                            }
                        }
                    }
                    Err(_) => continue,
                }
            }
        }
    } else {
        let json: ChatCompletionResponse = resp.into_body().read_json()?;
        if let Some(choice) = json.choices.first() {
            full_content = choice.message.content.clone();
            on_chunk(&full_content);
        }
    }

    Ok(full_content)
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

    if let Some(remaining) = resp
        .headers()
        .get("x-ratelimit-remaining-requests")
        .and_then(|v| v.to_str().ok())
    {
        let limit = resp
            .headers()
            .get("x-ratelimit-limit-requests")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("?");
        let usage_str = format!("{} / {}", remaining, limit);
        if let Ok(mut app) = APP.lock() {
            app.model_usage_stats.insert(p_model.to_string(), usage_str);
        }
    }

    let mut full_content = String::new();

    if streaming_enabled {
        let reader = BufReader::new(resp.into_body().into_reader());
        for line in reader.lines() {
            if let Some(ct) = cancel_token
                && ct.load(Ordering::Relaxed)
            {
                return Err(anyhow::anyhow!("Cancelled"));
            }
            let line = line?;
            if let Some(data) = line.strip_prefix("data: ") {
                if data == "[DONE]" {
                    break;
                }
                if let Ok(chunk) = serde_json::from_str::<StreamChunk>(data)
                    && let Some(content) =
                        chunk.choices.first().and_then(|c| c.delta.content.as_ref())
                {
                    full_content.push_str(content);
                    on_chunk(content);
                }
            }
        }
    } else {
        let json: ChatCompletionResponse = resp.into_body().read_json()?;
        if let Some(choice) = json.choices.first() {
            full_content = choice.message.content.clone();
            on_chunk(&full_content);
        }
    }

    Ok(full_content)
}
