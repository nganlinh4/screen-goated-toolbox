// --- GROQ TRANSLATE PROVIDERS ---
// Groq compound and standard API translation handlers.

use crate::APP;
use crate::api::client::UREQ_AGENT;
use crate::api::types::{ChatCompletionResponse, StreamChunk};
use crate::gui::locale::LocaleText;
use crate::overlay::utils::get_context_quote;
use anyhow::Result;
use std::io::{BufRead, BufReader};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

// --- GROQ COMPOUND MODEL ---
pub(super) fn translate_groq_compound<F>(
    groq_api_key: &str,
    model: &str,
    prompt: &str,
    search_label: Option<String>,
    ui_language: &str,
    mut on_chunk: F,
) -> Result<String>
where
    F: FnMut(&str),
{
    let payload = serde_json::json!({
        "model": model,
        "messages": [
            {
                "role": "system",
                "content": "IMPORTANT: Limit yourself to a maximum of 3 tool calls total. Make 1-2 focused searches, then answer. Do not visit websites unless absolutely necessary. Be efficient."
            },
            { "role": "user", "content": prompt }
        ],
        "temperature": 1,
        "max_tokens": 8192,
        "stream": false,
        "compound_custom": {
            "tools": {
                "enabled_tools": ["web_search", "visit_website"]
            }
        }
    });

    let locale = LocaleText::get(ui_language);
    let context_quote = get_context_quote(prompt);
    let search_msg = match &search_label {
        Some(label) => format!(
            "{}\n\n🔍 {} {}...",
            context_quote, locale.search_doing, label
        ),
        None => format!(
            "{}\n\n🔍 {} {}...",
            context_quote, locale.search_doing, locale.search_searching
        ),
    };
    on_chunk(&search_msg);

    let resp = UREQ_AGENT
        .post("https://api.groq.com/openai/v1/chat/completions")
        .header("Authorization", &format!("Bearer {}", groq_api_key))
        .send_json(payload)
        .map_err(|e| {
            let err_str = e.to_string();
            if err_str.contains("401") {
                anyhow::anyhow!("INVALID_API_KEY")
            } else {
                anyhow::anyhow!("{}", err_str)
            }
        })?;

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
            app.model_usage_stats.insert(model.to_string(), usage_str);
        }
    }

    let json: serde_json::Value = resp
        .into_body()
        .read_json()
        .map_err(|e| anyhow::anyhow!("Failed to parse compound response: {}", e))?;

    let mut full_content = String::new();

    if let Some(choices) = json.get("choices").and_then(|c| c.as_array())
        && let Some(first_choice) = choices.first()
        && let Some(message) = first_choice.get("message")
    {
        if let Some(executed_tools) = message.get("executed_tools").and_then(|t| t.as_array()) {
            let mut search_queries = Vec::new();
            for tool in executed_tools {
                if let Some(tool_type) = tool.get("type").and_then(|t| t.as_str())
                    && tool_type == "search"
                    && let Some(args) = tool.get("arguments").and_then(|a| a.as_str())
                    && let Ok(args_json) = serde_json::from_str::<serde_json::Value>(args)
                    && let Some(query) = args_json.get("query").and_then(|q| q.as_str())
                {
                    search_queries.push(query.to_string());
                }
            }

            let context_quote = get_context_quote(prompt);
            if !search_queries.is_empty() {
                let phase1_header = match &search_label {
                    Some(label) => format!(
                        "{}\n\n🔍 {} {}...\n\n",
                        context_quote,
                        locale.search_doing.to_uppercase(),
                        label.to_uppercase()
                    ),
                    None => format!(
                        "{}\n\n🔍 {} {}...\n\n",
                        context_quote,
                        locale.search_doing.to_uppercase(),
                        locale.search_searching.to_uppercase()
                    ),
                };
                let mut phase1 = phase1_header;
                phase1.push_str(&format!("{}\n", locale.search_query_label));
                for (i, query) in search_queries.iter().enumerate() {
                    phase1.push_str(&format!("  {}. \"{}\"\n", i + 1, query));
                }
                on_chunk(&phase1);
                std::thread::sleep(std::time::Duration::from_millis(800));
            }

            let mut all_sources = Vec::new();
            for tool in executed_tools {
                if let Some(search_results) = tool
                    .get("search_results")
                    .and_then(|s| s.get("results"))
                    .and_then(|r| r.as_array())
                {
                    for result in search_results {
                        let title = result
                            .get("title")
                            .and_then(|t| t.as_str())
                            .unwrap_or(locale.search_no_title);
                        let url = result.get("url").and_then(|u| u.as_str()).unwrap_or("");
                        let score = result.get("score").and_then(|s| s.as_f64()).unwrap_or(0.0);
                        let content = result.get("content").and_then(|c| c.as_str()).unwrap_or("");

                        all_sources.push((
                            title.to_string(),
                            url.to_string(),
                            score,
                            content.to_string(),
                        ));
                    }
                }
            }

            if !all_sources.is_empty() {
                all_sources
                    .sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

                let context_quote = get_context_quote(prompt);
                let mut phase2 = format!(
                    "{}\n\n{}\n\n",
                    context_quote,
                    locale
                        .search_found_sources
                        .replace("{}", &all_sources.len().to_string())
                );
                phase2.push_str(&format!("{}\n\n", locale.search_sources_label));

                for (i, (title, url, score, content)) in all_sources.iter().take(6).enumerate() {
                    let title_display = if title.chars().count() > 60 {
                        format!("{}...", title.chars().take(57).collect::<String>())
                    } else {
                        title.clone()
                    };

                    let domain = url.split('/').nth(2).unwrap_or(url);
                    let score_pct = (score * 100.0) as i32;

                    phase2.push_str(&format!("{}. {} [{}%]\n", i + 1, title_display, score_pct));
                    phase2.push_str(&format!("   🔗 {}\n", domain));

                    if !content.is_empty() {
                        let preview = if content.len() > 100 {
                            format!(
                                "{}...",
                                content
                                    .chars()
                                    .take(100)
                                    .collect::<String>()
                                    .replace('\n', " ")
                            )
                        } else {
                            content.replace('\n', " ")
                        };
                        phase2.push_str(&format!("   📄 {}\n", preview));
                    }
                    phase2.push('\n');
                }

                on_chunk(&phase2);
                std::thread::sleep(std::time::Duration::from_millis(1200));

                let context_quote = get_context_quote(prompt);
                let phase3 = format!(
                    "{}\n\n{}\n\n{}\n{}\n",
                    context_quote,
                    locale.search_synthesizing,
                    locale
                        .search_analyzed_sources
                        .replace("{}", &all_sources.len().min(6).to_string()),
                    locale.search_processing
                );
                on_chunk(&phase3);
                std::thread::sleep(std::time::Duration::from_millis(600));
            }
        }

        if let Some(content) = message.get("content").and_then(|c| c.as_str()) {
            full_content = content.to_string();
            on_chunk(&full_content);
        }
    }

    Ok(full_content)
}

// --- GROQ STANDARD API ---
pub(super) fn translate_groq_standard<F>(
    groq_api_key: &str,
    model: &str,
    prompt: &str,
    streaming_enabled: bool,
    use_json_format: bool,
    cancel_token: Option<Arc<AtomicBool>>,
    mut on_chunk: F,
) -> Result<String>
where
    F: FnMut(&str),
{
    let payload = if streaming_enabled {
        serde_json::json!({
            "model": model,
            "messages": [
                { "role": "user", "content": prompt }
            ],
            "stream": true
        })
    } else {
        let mut payload_obj = serde_json::json!({
            "model": model,
            "messages": [
                { "role": "user", "content": prompt }
            ],
            "stream": false
        });

        if use_json_format {
            payload_obj["response_format"] = serde_json::json!({ "type": "json_object" });
        }

        payload_obj
    };

    let resp = UREQ_AGENT
        .post("https://api.groq.com/openai/v1/chat/completions")
        .header("Authorization", &format!("Bearer {}", groq_api_key))
        .send_json(payload)
        .map_err(|e| {
            let err_str = e.to_string();
            if err_str.contains("401") {
                anyhow::anyhow!("INVALID_API_KEY")
            } else {
                anyhow::anyhow!("{}", err_str)
            }
        })?;

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
            app.model_usage_stats.insert(model.to_string(), usage_str);
        }
    }

    let mut full_content = String::new();

    if streaming_enabled {
        let reader = BufReader::new(resp.into_body().into_reader());

        for line in reader.lines() {
            if let Some(ref ct) = cancel_token
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
                        if let Some(content) =
                            chunk.choices.first().and_then(|c| c.delta.content.as_ref())
                        {
                            full_content.push_str(content);
                            on_chunk(content);
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
            let content_str = &choice.message.content;

            if use_json_format {
                if let Ok(json_obj) = serde_json::from_str::<serde_json::Value>(content_str) {
                    if let Some(translation) = json_obj.get("translation").and_then(|v| v.as_str())
                    {
                        full_content = translation.to_string();
                    } else {
                        full_content = content_str.clone();
                    }
                } else {
                    full_content = content_str.clone();
                }
            } else {
                full_content = content_str.clone();
            }

            on_chunk(&full_content);
        }
    }

    Ok(full_content)
}
