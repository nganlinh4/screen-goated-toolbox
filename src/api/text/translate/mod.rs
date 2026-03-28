// --- TEXT TRANSLATION ---
// Streaming text translation with multiple LLM providers.

mod groq;

use crate::APP;
use crate::api::client::UREQ_AGENT;
use crate::api::types::{ChatCompletionResponse, StreamChunk};
use crate::gui::locale::LocaleText;
use anyhow::Result;
use std::io::{BufRead, BufReader};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

pub struct TranslateTextRequest<'a> {
    pub groq_api_key: &'a str,
    pub gemini_api_key: &'a str,
    pub text: String,
    pub instruction: String,
    pub model: String,
    pub provider: String,
    pub streaming_enabled: bool,
    pub use_json_format: bool,
    pub search_label: Option<String>,
    pub ui_language: &'a str,
    pub cancel_token: Option<Arc<AtomicBool>>,
}

pub fn translate_text_streaming<F>(
    request: TranslateTextRequest<'_>,
    mut on_chunk: F,
) -> Result<String>
where
    F: FnMut(&str),
{
    let TranslateTextRequest {
        groq_api_key,
        gemini_api_key,
        text,
        instruction,
        model,
        provider,
        streaming_enabled,
        use_json_format,
        search_label,
        ui_language,
        cancel_token,
    } = request;

    let openrouter_api_key = crate::APP
        .lock()
        .ok()
        .and_then(|app| {
            let config = app.config.clone();
            if config.openrouter_api_key.is_empty() {
                None
            } else {
                Some(config.openrouter_api_key.clone())
            }
        })
        .unwrap_or_default();

    let cerebras_api_key = crate::APP
        .lock()
        .ok()
        .and_then(|app| {
            let config = app.config.clone();
            if config.cerebras_api_key.is_empty() {
                None
            } else {
                Some(config.cerebras_api_key.clone())
            }
        })
        .unwrap_or_default();

    let full_content;
    let prompt = format!("{}\n\n{}", instruction, text);

    // DEBUG: Log the instruction being sent to the API
    println!("[DEBUG translate] instruction=«{}»", instruction);
    println!(
        "[DEBUG translate] text_len={} prompt_len={}",
        text.len(),
        prompt.len()
    );

    if provider == "ollama" {
        // --- OLLAMA LOCAL API ---
        let (ollama_base_url, ollama_text_model) = crate::APP
            .lock()
            .ok()
            .map(|app| {
                let config = app.config.clone();
                (
                    config.ollama_base_url.clone(),
                    config.ollama_text_model.clone(),
                )
            })
            .unwrap_or_else(|| ("http://localhost:11434".to_string(), model.clone()));

        let actual_model = if ollama_text_model.is_empty() {
            model.clone()
        } else {
            ollama_text_model
        };

        return crate::api::ollama::ollama_generate_text(
            &ollama_base_url,
            &actual_model,
            &prompt,
            streaming_enabled,
            ui_language,
            on_chunk,
        );
    } else if provider == "gemini-live" {
        // --- GEMINI LIVE API (WebSocket-based low-latency streaming) ---
        return crate::api::gemini_live::gemini_live_generate(
            model,
            text,
            instruction,
            None, // No image for text-only
            None, // No audio for text-only
            streaming_enabled,
            ui_language,
            on_chunk,
        );
    } else if provider == "google-gtx" {
        // --- GOOGLE TRANSLATE (GTX) API ---
        let target_lang = instruction
            .to_lowercase()
            .split("translate to ")
            .nth(1)
            .and_then(|s| s.split('.').next())
            .and_then(|s| s.split(',').next())
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "English".to_string());

        let target_lang = target_lang
            .chars()
            .enumerate()
            .map(|(i, c)| {
                if i == 0 {
                    c.to_uppercase().next().unwrap_or(c)
                } else {
                    c
                }
            })
            .collect::<String>();

        match crate::api::realtime_audio::translate_with_google_gtx(&text, &target_lang) {
            Some(translated) => {
                on_chunk(&translated);
                return Ok(translated);
            }
            None => {
                return Err(anyhow::anyhow!("GTX translation failed"));
            }
        }
    } else if provider == "taalas" {
        // --- TAALAS API (chatjimmy.ai / HC1 silicon) ---
        full_content = translate_taalas(&prompt, &cancel_token, &mut on_chunk)?;
    } else if provider == "google" {
        // --- GEMINI TEXT API ---
        full_content = translate_gemini(
            gemini_api_key,
            &model,
            &prompt,
            streaming_enabled,
            ui_language,
            &cancel_token,
            &mut on_chunk,
        )?;
    } else if provider == "cerebras" {
        // --- CEREBRAS API ---
        full_content = translate_cerebras(
            &cerebras_api_key,
            &model,
            &prompt,
            streaming_enabled,
            ui_language,
            &cancel_token,
            &mut on_chunk,
        )?;
    } else if provider == "openrouter" {
        // --- OPENROUTER API ---
        full_content = translate_openrouter(
            &openrouter_api_key,
            &model,
            &prompt,
            streaming_enabled,
            ui_language,
            &cancel_token,
            &mut on_chunk,
        )?;
    } else {
        // --- GROQ API (Default) ---
        if groq_api_key.trim().is_empty() {
            return Err(anyhow::anyhow!("NO_API_KEY:groq"));
        }

        let is_compound = model.starts_with("groq/compound");

        if is_compound {
            return groq::translate_groq_compound(
                groq_api_key,
                &model,
                &prompt,
                search_label,
                ui_language,
                on_chunk,
            );
        } else {
            return groq::translate_groq_standard(
                groq_api_key,
                &model,
                &prompt,
                streaming_enabled,
                use_json_format,
                cancel_token,
                on_chunk,
            );
        }
    }

    Ok(full_content)
}

// --- GEMINI TEXT API ---
fn translate_gemini<F>(
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

    let method = if streaming_enabled {
        "streamGenerateContent"
    } else {
        "generateContent"
    };
    let url = if streaming_enabled {
        format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:{}?alt=sse",
            model, method
        )
    } else {
        format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:{}",
            model, method
        )
    };

    let mut payload = serde_json::json!({
        "contents": [{
            "role": "user",
            "parts": [{ "text": prompt }]
        }]
    });

    if let Some(thinking_config) = crate::api::gemini_thinking_config(model) {
        payload["generationConfig"] = serde_json::json!({
            "thinkingConfig": thinking_config
        });
    }

    if crate::model_config::model_supports_search_by_name(model) {
        payload["tools"] = serde_json::json!([
            { "url_context": {} },
            { "google_search": {} }
        ]);
    }

    let resp = UREQ_AGENT
        .post(&url)
        .header("x-goog-api-key", gemini_api_key)
        .send_json(payload)
        .map_err(|e| {
            let err_str = e.to_string();
            if err_str.contains("401") || err_str.contains("403") {
                anyhow::anyhow!("INVALID_API_KEY")
            } else {
                anyhow::anyhow!("Gemini Text API Error: {}", err_str)
            }
        })?;

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
            let line = line.map_err(|e| anyhow::anyhow!("Failed to read line: {}", e))?;
            if let Some(json_str) = line.strip_prefix("data: ") {
                if json_str.trim() == "[DONE]" {
                    break;
                }

                if let Ok(chunk_resp) = serde_json::from_str::<serde_json::Value>(json_str)
                    && let Some(candidates) =
                        chunk_resp.get("candidates").and_then(|c| c.as_array())
                    && let Some(first_candidate) = candidates.first()
                    && let Some(parts) = first_candidate
                        .get("content")
                        .and_then(|c| c.get("parts"))
                        .and_then(|p| p.as_array())
                {
                    for part in parts {
                        let is_thought = part
                            .get("thought")
                            .and_then(|t| t.as_bool())
                            .unwrap_or(false);

                        if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                            if is_thought {
                                if !thinking_shown && !content_started {
                                    on_chunk(locale.model_thinking);
                                    thinking_shown = true;
                                }
                            } else if !content_started && thinking_shown {
                                content_started = true;
                                full_content.push_str(text);
                                let wipe_content =
                                    format!("{}{}", crate::api::WIPE_SIGNAL, full_content);
                                on_chunk(&wipe_content);
                            } else {
                                content_started = true;
                                full_content.push_str(text);
                                on_chunk(text);
                            }
                        }
                    }
                }
            }
        }
    } else {
        let chat_resp: serde_json::Value = resp
            .into_body()
            .read_json()
            .map_err(|e| anyhow::anyhow!("Failed to parse non-streaming response: {}", e))?;

        if let Some(candidates) = chat_resp.get("candidates").and_then(|c| c.as_array())
            && let Some(first_choice) = candidates.first()
            && let Some(parts) = first_choice
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

// --- CEREBRAS API ---
fn translate_cerebras<F>(
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

    let payload = serde_json::json!({
        "model": model,
        "messages": [
            { "role": "user", "content": prompt }
        ],
        "stream": streaming_enabled
    });

    let resp = UREQ_AGENT
        .post("https://api.cerebras.ai/v1/chat/completions")
        .header("Authorization", &format!("Bearer {}", cerebras_api_key))
        .header("Content-Type", "application/json")
        .send_json(payload)
        .map_err(|e| {
            let err_str = e.to_string();
            if err_str.contains("401") || err_str.contains("403") {
                anyhow::anyhow!("INVALID_API_KEY")
            } else {
                anyhow::anyhow!("Cerebras API Error: {}", err_str)
            }
        })?;

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
        && let Some(conf) = crate::model_config::get_model_by_id(model)
        && let Some(val) = conf.quota_limit_en.split_whitespace().next()
    {
        limit = val.to_string();
    }

    if remaining != "?" || limit != "?" {
        let usage_str = format!("{} / {}", remaining, limit);
        if let Ok(mut app) = APP.lock() {
            app.model_usage_stats.insert(model.to_string(), usage_str);
        }
    }

    let mut full_content = String::new();

    if streaming_enabled {
        let reader = BufReader::new(resp.into_body().into_reader());
        let mut thinking_shown = false;
        let mut content_started = false;
        let locale = LocaleText::get(ui_language);

        let is_reasoning_model = model.contains("gpt-oss") || model.contains("zai-glm");

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
        let chat_resp: ChatCompletionResponse = resp
            .into_body()
            .read_json()
            .map_err(|e| anyhow::anyhow!("Failed to parse non-streaming response: {}", e))?;

        if let Some(choice) = chat_resp.choices.first() {
            full_content = choice.message.content.clone();
            on_chunk(&full_content);
        }
    }

    Ok(full_content)
}

// --- TAALAS API ---
fn translate_taalas<F>(
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
fn translate_openrouter<F>(
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

    let payload = serde_json::json!({
        "model": model,
        "messages": [
            { "role": "user", "content": prompt }
        ],
        "stream": streaming_enabled
    });

    let resp = UREQ_AGENT
        .post("https://openrouter.ai/api/v1/chat/completions")
        .header("Authorization", &format!("Bearer {}", openrouter_api_key))
        .header("Content-Type", "application/json")
        .send_json(payload)
        .map_err(|e| {
            let err_str = e.to_string();
            if err_str.contains("401") || err_str.contains("403") {
                anyhow::anyhow!("INVALID_API_KEY")
            } else {
                anyhow::anyhow!("OpenRouter API Error: {}", err_str)
            }
        })?;

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
        let chat_resp: ChatCompletionResponse = resp
            .into_body()
            .read_json()
            .map_err(|e| anyhow::anyhow!("Failed to parse non-streaming response: {}", e))?;

        if let Some(choice) = chat_resp.choices.first() {
            full_content = choice.message.content.clone();
            on_chunk(&full_content);
        }
    }

    Ok(full_content)
}
