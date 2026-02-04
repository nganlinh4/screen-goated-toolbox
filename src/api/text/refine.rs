// --- TEXT REFINEMENT ---
// Streaming text refinement with multiple LLM providers.

use crate::api::client::UREQ_AGENT;
use crate::api::types::{ChatCompletionResponse, StreamChunk};
use crate::api::vision::translate_image_streaming as vision_translate_image_streaming;
use crate::gui::locale::LocaleText;
use crate::overlay::result::RefineContext;
use crate::overlay::utils::get_context_quote;
use crate::APP;
use anyhow::Result;
use std::io::{BufRead, BufReader};

pub fn refine_text_streaming<F>(
    groq_api_key: &str,
    gemini_api_key: &str,
    context: RefineContext,
    previous_text: String,
    user_prompt: String,
    original_model_id: &str,
    original_provider: &str,
    streaming_enabled: bool,
    ui_language: &str,
    mut on_chunk: F,
) -> Result<String>
where
    F: FnMut(&str),
{
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

    let final_prompt = format!(
        "Content:\n{}\n\nInstruction:\n{}\n\nOutput ONLY the result.",
        previous_text, user_prompt
    );

    let (mut target_id_or_name, mut target_provider) = match context {
        RefineContext::Image(_) => (original_model_id.to_string(), original_provider.to_string()),
        _ => {
            if !original_model_id.trim().is_empty() && original_model_id != "scout" {
                (original_model_id.to_string(), original_provider.to_string())
            } else if !gemini_api_key.trim().is_empty() {
                ("gemini-flash-lite".to_string(), "google".to_string())
            } else if !cerebras_api_key.trim().is_empty() {
                (
                    "qwen-3-235b-a22b-instruct-2507".to_string(),
                    "cerebras".to_string(),
                )
            } else if !groq_api_key.trim().is_empty() {
                ("text_accurate_kimi".to_string(), "groq".to_string())
            } else {
                (original_model_id.to_string(), original_provider.to_string())
            }
        }
    };

    if let Some(conf) = crate::model_config::get_model_by_id(&target_id_or_name) {
        target_id_or_name = conf.full_name;
        target_provider = conf.provider;
    }

    let mut exec_text_only = |p_model: String, p_provider: String| -> Result<String> {
        refine_text_only(
            groq_api_key,
            gemini_api_key,
            &openrouter_api_key,
            &cerebras_api_key,
            &final_prompt,
            p_model,
            p_provider,
            streaming_enabled,
            ui_language,
            &mut on_chunk,
        )
    };

    match context {
        RefineContext::Image(img_bytes) => {
            if target_provider == "google" {
                if gemini_api_key.trim().is_empty() {
                    return Err(anyhow::anyhow!("NO_API_KEY:gemini"));
                }
                let img = image::load_from_memory(&img_bytes)?.to_rgba8();
                vision_translate_image_streaming(
                    groq_api_key,
                    gemini_api_key,
                    final_prompt,
                    target_id_or_name,
                    target_provider,
                    img,
                    Some(img_bytes.clone()),
                    streaming_enabled,
                    false,
                    on_chunk,
                )
            } else if target_provider == "gemini-live" {
                let mime = "image/jpeg".to_string();
                crate::api::gemini_live::gemini_live_generate(
                    final_prompt,
                    String::new(),
                    Some((img_bytes.clone(), mime)),
                    None,
                    streaming_enabled,
                    ui_language,
                    &mut on_chunk,
                )
            } else {
                if groq_api_key.trim().is_empty() {
                    return Err(anyhow::anyhow!("NO_API_KEY:groq"));
                }
                let img = image::load_from_memory(&img_bytes)?.to_rgba8();
                vision_translate_image_streaming(
                    groq_api_key,
                    gemini_api_key,
                    final_prompt,
                    target_id_or_name,
                    target_provider,
                    img,
                    Some(img_bytes.clone()),
                    streaming_enabled,
                    false,
                    on_chunk,
                )
            }
        }
        RefineContext::Audio(_) => exec_text_only(target_id_or_name, target_provider),
        RefineContext::None => exec_text_only(target_id_or_name, target_provider),
    }
}

// --- TEXT-ONLY REFINEMENT ---
fn refine_text_only<F>(
    groq_api_key: &str,
    gemini_api_key: &str,
    openrouter_api_key: &str,
    cerebras_api_key: &str,
    final_prompt: &str,
    p_model: String,
    p_provider: String,
    streaming_enabled: bool,
    ui_language: &str,
    on_chunk: &mut F,
) -> Result<String>
where
    F: FnMut(&str),
{
    if p_provider == "google" {
        refine_gemini(
            gemini_api_key,
            final_prompt,
            &p_model,
            streaming_enabled,
            ui_language,
            on_chunk,
        )
    } else if p_provider == "gemini-live" {
        crate::api::gemini_live::gemini_live_generate(
            final_prompt.to_string(),
            String::new(),
            None,
            None,
            streaming_enabled,
            ui_language,
            on_chunk,
        )
    } else if p_provider == "cerebras" {
        refine_cerebras(
            cerebras_api_key,
            final_prompt,
            &p_model,
            streaming_enabled,
            ui_language,
            on_chunk,
        )
    } else if p_provider == "openrouter" {
        refine_openrouter(
            openrouter_api_key,
            final_prompt,
            &p_model,
            streaming_enabled,
            ui_language,
            on_chunk,
        )
    } else {
        refine_groq(
            groq_api_key,
            final_prompt,
            &p_model,
            streaming_enabled,
            ui_language,
            on_chunk,
        )
    }
}

// --- GEMINI REFINE ---
fn refine_gemini<F>(
    gemini_api_key: &str,
    final_prompt: &str,
    p_model: &str,
    streaming_enabled: bool,
    ui_language: &str,
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

    let supports_thinking = (p_model.contains("gemini-2.5-flash") && !p_model.contains("lite"))
        || p_model.contains("gemini-3-flash-preview")
        || p_model.contains("gemini-robotics");
    if supports_thinking {
        payload["generationConfig"] = serde_json::json!({
            "thinkingConfig": {
                "includeThoughts": true
            }
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
            let line = line?;
            if line.starts_with("data: ") {
                let json_str = &line["data: ".len()..];
                if json_str.trim() == "[DONE]" {
                    break;
                }
                if let Ok(chunk_resp) = serde_json::from_str::<serde_json::Value>(json_str) {
                    if let Some(candidates) =
                        chunk_resp.get("candidates").and_then(|c| c.as_array())
                    {
                        if let Some(first) = candidates.first() {
                            if let Some(parts) = first
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
                                            let wipe_content = format!(
                                                "{}{}",
                                                crate::api::WIPE_SIGNAL,
                                                full_content
                                            );
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
                }
            }
        }
    } else {
        let json: serde_json::Value = resp.into_body().read_json()?;
        if let Some(candidates) = json.get("candidates").and_then(|c| c.as_array()) {
            if let Some(first) = candidates.first() {
                if let Some(parts) = first
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
        }
    }

    Ok(full_content)
}

// --- CEREBRAS REFINE ---
fn refine_cerebras<F>(
    cerebras_api_key: &str,
    final_prompt: &str,
    p_model: &str,
    streaming_enabled: bool,
    ui_language: &str,
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

    if limit == "?" {
        if let Some(conf) = crate::model_config::get_model_by_id(p_model) {
            if let Some(val) = conf.quota_limit_en.split_whitespace().next() {
                limit = val.to_string();
            }
        }
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
            let line = line?;
            if line.starts_with("data: ") {
                let data = &line[6..];
                if data == "[DONE]" {
                    break;
                }

                match serde_json::from_str::<StreamChunk>(data) {
                    Ok(chunk) => {
                        if let Some(reasoning) = chunk
                            .choices
                            .get(0)
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
                            .get(0)
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
fn refine_openrouter<F>(
    openrouter_api_key: &str,
    final_prompt: &str,
    p_model: &str,
    streaming_enabled: bool,
    ui_language: &str,
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
            let line = line?;
            if line.starts_with("data: ") {
                let data = &line[6..];
                if data == "[DONE]" {
                    break;
                }

                match serde_json::from_str::<StreamChunk>(data) {
                    Ok(chunk) => {
                        if let Some(reasoning) = chunk
                            .choices
                            .get(0)
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
                            .get(0)
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
fn refine_groq<F>(
    groq_api_key: &str,
    final_prompt: &str,
    p_model: &str,
    streaming_enabled: bool,
    ui_language: &str,
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
            let line = line?;
            if line.starts_with("data: ") {
                let data = &line[6..];
                if data == "[DONE]" {
                    break;
                }
                if let Ok(chunk) = serde_json::from_str::<StreamChunk>(data) {
                    if let Some(content) = chunk.choices.get(0).and_then(|c| c.delta.content.as_ref())
                    {
                        full_content.push_str(content);
                        on_chunk(content);
                    }
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

// --- GROQ COMPOUND REFINE ---
fn refine_groq_compound<F>(
    groq_api_key: &str,
    final_prompt: &str,
    p_model: &str,
    ui_language: &str,
    on_chunk: &mut F,
) -> Result<String>
where
    F: FnMut(&str),
{
    let payload = serde_json::json!({
        "model": p_model,
        "messages": [
            {
                "role": "system",
                "content": "IMPORTANT: Limit yourself to a maximum of 3 tool calls total. Make 1-2 focused searches, then answer. Do not visit websites unless absolutely necessary. Be efficient."
            },
            { "role": "user", "content": final_prompt }
        ],
        "temperature": 1,
        "max_completion_tokens": 8192,
        "stream": false,
        "compound_custom": {
            "tools": {
                "enabled_tools": ["web_search", "visit_website"]
            }
        }
    });

    let locale = LocaleText::get(ui_language);
    let context_quote = get_context_quote(final_prompt);
    on_chunk(&format!(
        "{}\n\n🔍 {} {}...",
        context_quote, locale.search_doing, locale.search_searching
    ));

    let resp = UREQ_AGENT
        .post("https://api.groq.com/openai/v1/chat/completions")
        .header("Authorization", &format!("Bearer {}", groq_api_key))
        .send_json(payload)
        .map_err(|e| anyhow::anyhow!("Groq Compound Refine Error: {}", e))?;

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

    let json: serde_json::Value = resp.into_body().read_json()?;
    let mut full_content = String::new();

    if let Some(choices) = json.get("choices").and_then(|c| c.as_array()) {
        if let Some(first_choice) = choices.first() {
            if let Some(message) = first_choice.get("message") {
                if let Some(executed_tools) =
                    message.get("executed_tools").and_then(|t| t.as_array())
                {
                    let mut search_queries = Vec::new();
                    for tool in executed_tools {
                        if tool.get("type").and_then(|t| t.as_str()) == Some("search") {
                            if let Some(args) = tool.get("arguments").and_then(|a| a.as_str()) {
                                if let Ok(args_json) =
                                    serde_json::from_str::<serde_json::Value>(args)
                                {
                                    if let Some(query) =
                                        args_json.get("query").and_then(|q| q.as_str())
                                    {
                                        search_queries.push(query.to_string());
                                    }
                                }
                            }
                        }
                    }

                    if !search_queries.is_empty() {
                        let context_quote = get_context_quote(final_prompt);
                        let mut phase1 = format!(
                            "{}\n\n🔍 {} {}...\n\n{}\n",
                            context_quote,
                            locale.search_doing.to_uppercase(),
                            locale.search_searching.to_uppercase(),
                            locale.search_query_label
                        );
                        for (i, q) in search_queries.iter().enumerate() {
                            phase1.push_str(&format!("  {}. \"{}\"\n", i + 1, q));
                        }
                        on_chunk(&phase1);
                        std::thread::sleep(std::time::Duration::from_millis(600));
                    }

                    let mut all_sources = Vec::new();
                    for tool in executed_tools {
                        if let Some(results) = tool
                            .get("search_results")
                            .and_then(|s| s.get("results"))
                            .and_then(|r| r.as_array())
                        {
                            for r in results {
                                let title = r
                                    .get("title")
                                    .and_then(|t| t.as_str())
                                    .unwrap_or(locale.search_no_title);
                                let url = r.get("url").and_then(|u| u.as_str()).unwrap_or("");
                                let score = r.get("score").and_then(|s| s.as_f64()).unwrap_or(0.0);
                                all_sources.push((title.to_string(), url.to_string(), score));
                            }
                        }
                    }

                    if !all_sources.is_empty() {
                        all_sources
                            .sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
                        let context_quote = get_context_quote(final_prompt);
                        let mut phase2 = format!(
                            "{}\n\n{}\n\n",
                            context_quote,
                            locale
                                .search_found_sources
                                .replace("{}", &all_sources.len().to_string())
                        );
                        for (i, (title, url, score)) in all_sources.iter().take(5).enumerate() {
                            let t = if title.chars().count() > 50 {
                                format!("{}...", title.chars().take(47).collect::<String>())
                            } else {
                                title.clone()
                            };
                            let domain = url.split('/').nth(2).unwrap_or("");
                            phase2.push_str(&format!(
                                "{}. {} [{}%]\n   🔗 {}\n",
                                i + 1,
                                t,
                                (score * 100.0) as i32,
                                domain
                            ));
                        }
                        phase2.push_str(&format!("\n{}", locale.search_synthesizing));
                        on_chunk(&phase2);
                        std::thread::sleep(std::time::Duration::from_millis(800));
                    }
                }

                if let Some(content) = message.get("content").and_then(|c| c.as_str()) {
                    full_content = content.to_string();
                    on_chunk(&full_content);
                }
            }
        }
    }

    Ok(full_content)
}
