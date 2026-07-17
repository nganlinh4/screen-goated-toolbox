// --- GROQ TRANSLATE PROVIDERS ---
// Groq compound and standard API translation handlers.

use crate::api::client::{UREQ_RESPONSE_AGENT, record_groq_json_usage, record_usage_simple};
use crate::api::types::ChatCompletionResponse;
use crate::gui::locale::LocaleText;
use crate::overlay::utils::get_context_quote;
use anyhow::Result;
use std::io::BufReader;
use std::time::Duration;

use super::TranslateTransportOptions;

// --- GROQ COMPOUND MODEL ---
pub(super) fn translate_groq_compound<F>(
    groq_api_key: &str,
    model: &str,
    prompt: &str,
    search_label: Option<String>,
    ui_language: &str,
    request_timeout: Option<Duration>,
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
            context_quote, locale.workspace.search_doing, label
        ),
        None => format!(
            "{}\n\n🔍 {} {}...",
            context_quote, locale.workspace.search_doing, locale.workspace.search_searching
        ),
    };
    on_chunk(&search_msg);

    let request = UREQ_RESPONSE_AGENT
        .post("https://api.groq.com/openai/v1/chat/completions")
        .header("Authorization", &format!("Bearer {}", groq_api_key));
    let resp = crate::api::client::with_request_timeout(request, request_timeout)
        .send_json(payload)
        .map_err(|error| anyhow::anyhow!("Groq transport error: {error}"))?;
    let resp = require_success(resp)?;

    record_usage_simple(resp.headers(), model);

    let json: serde_json::Value = resp
        .into_body()
        .read_json()
        .map_err(|e| anyhow::anyhow!("Failed to parse compound response: {}", e))?;
    record_groq_json_usage(model, &json);

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
                        locale.workspace.search_doing.to_uppercase(),
                        label.to_uppercase()
                    ),
                    None => format!(
                        "{}\n\n🔍 {} {}...\n\n",
                        context_quote,
                        locale.workspace.search_doing.to_uppercase(),
                        locale.workspace.search_searching.to_uppercase()
                    ),
                };
                let mut phase1 = phase1_header;
                phase1.push_str(&format!("{}\n", locale.workspace.search_query_label));
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
                            .unwrap_or(locale.workspace.search_no_title);
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
                        .workspace
                        .search_found_sources
                        .replace("{}", &all_sources.len().to_string())
                );
                phase2.push_str(&format!("{}\n\n", locale.workspace.search_sources_label));

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
                    locale.workspace.search_synthesizing,
                    locale
                        .workspace
                        .search_analyzed_sources
                        .replace("{}", &all_sources.len().min(6).to_string()),
                    locale.workspace.search_processing
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
    use_json_format: bool,
    transport: TranslateTransportOptions<'_>,
    mut on_chunk: F,
) -> Result<String>
where
    F: FnMut(&str),
{
    let payload = if transport.streaming_enabled {
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
            payload_obj["response_format"] = crate::api::groq::structured_response_format(
                model,
                "translation_result",
                serde_json::json!({
                    "type": "object",
                    "properties": { "translation": { "type": "string" } },
                    "required": ["translation"],
                    "additionalProperties": false
                }),
            );
        }

        payload_obj
    };

    let request = UREQ_RESPONSE_AGENT
        .post("https://api.groq.com/openai/v1/chat/completions")
        .header("Authorization", &format!("Bearer {}", groq_api_key));
    let resp = crate::api::client::with_request_timeout(request, transport.request_timeout)
        .send_json(payload)
        .map_err(|error| anyhow::anyhow!("Groq transport error: {error}"))?;
    let resp = require_success(resp)?;

    record_usage_simple(resp.headers(), model);

    let mut full_content = String::new();

    if transport.streaming_enabled {
        let reader = BufReader::new(resp.into_body().into_reader());
        full_content = crate::api::openai_compat::consume_content_stream(
            reader,
            transport.cancel_token,
            &mut on_chunk,
        )?;
    } else {
        let root: serde_json::Value = resp
            .into_body()
            .read_json()
            .map_err(|e| anyhow::anyhow!("Failed to parse non-streaming response: {}", e))?;
        record_groq_json_usage(model, &root);
        let chat_resp: ChatCompletionResponse = serde_json::from_value(root)
            .map_err(|e| anyhow::anyhow!("Failed to decode non-streaming response: {}", e))?;

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

fn require_success(
    response: ureq::http::Response<ureq::Body>,
) -> Result<ureq::http::Response<ureq::Body>> {
    let status = response.status().as_u16();
    if response.status().is_success() {
        return Ok(response);
    }
    if matches!(status, 401 | 403) {
        return Err(anyhow::anyhow!("INVALID_API_KEY"));
    }
    let body = response.into_body().read_to_string().unwrap_or_default();
    Err(anyhow::anyhow!(
        "Groq API HTTP {status}: {}",
        groq_error_message(status, &body)
    ))
}

fn groq_error_message(status: u16, body: &str) -> String {
    let message = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|root| {
            root.pointer("/error/message")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| format!("request failed with status {status}"));
    let compact = message.split_whitespace().collect::<Vec<_>>().join(" ");
    compact.chars().take(500).collect()
}

#[cfg(test)]
mod tests {
    use super::groq_error_message;

    #[test]
    fn structured_error_keeps_bounded_provider_reason() {
        let body = serde_json::json!({
            "error": {"message": format!("payload too large: {}", "x".repeat(800))}
        })
        .to_string();
        let message = groq_error_message(413, &body);
        assert!(message.starts_with("payload too large:"));
        assert_eq!(message.chars().count(), 500);
    }

    #[test]
    fn malformed_error_body_reports_only_the_status() {
        assert_eq!(
            groq_error_message(429, "not-json"),
            "request failed with status 429"
        );
    }
}
