//! Groq compound models run server-side web search before answering. One
//! implementation serves translate and refine, so both get the request budget,
//! Stop during progress phases and phases that replace one another.

use crate::api::client::{UREQ_RESPONSE_AGENT, record_groq_json_usage, record_usage_simple};
use crate::gui::locale::LocaleText;
use crate::overlay::utils::get_context_quote;
use anyhow::Result;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

pub(crate) struct CompoundRequest<'a> {
    pub endpoint: &'a str,
    pub api_key: &'a str,
    pub model: &'a str,
    pub prompt: &'a str,
    /// Shown in the search progress line; `None` uses the generic label.
    pub search_label: Option<String>,
    pub ui_language: &'a str,
    pub request_timeout: Option<crate::api::client::RequestTimeouts>,
    pub cancel_token: &'a Option<Arc<AtomicBool>>,
}

pub(crate) fn run_compound<F>(request: CompoundRequest<'_>, mut on_chunk: F) -> Result<String>
where
    F: FnMut(&str),
{
    let CompoundRequest {
        endpoint,
        api_key: groq_api_key,
        model,
        prompt,
        search_label,
        ui_language,
        request_timeout,
        cancel_token,
    } = request;
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
        "max_completion_tokens": 8192,
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
        .post(endpoint)
        .header("Authorization", &format!("Bearer {}", groq_api_key));
    let resp = crate::api::client::with_request_timeouts(request, request_timeout)
        .send_json(payload)
        .map_err(|error| {
            anyhow::anyhow!(crate::api::client::transport_error_message(
                "Groq transport error",
                &error,
            ))
        })?;
    record_usage_simple(resp.headers(), model);
    let status = resp.status().as_u16();
    if !resp.status().is_success() {
        let body = resp.into_body().read_to_string().unwrap_or_default();
        return Err(super::http_error(status, &body));
    }

    let json: serde_json::Value = resp.into_body().read_json().map_err(|e| {
        anyhow::anyhow!(
            "PROVIDER_RESPONSE_INVALID:Malformed provider completion: {}",
            e
        )
    })?;
    record_groq_json_usage(model, &json);

    let full_content = crate::api::openai_compat::parse_chat_completion(&json)?;

    if let Some(choices) = json.get("choices").and_then(|c| c.as_array())
        && let Some(first_choice) = choices.first()
        && let Some(message) = first_choice.get("message")
        && let Some(executed_tools) = message.get("executed_tools").and_then(|t| t.as_array())
    {
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
            show_phase(&mut on_chunk, &phase1);
            hold_phase(Duration::from_millis(800), cancel_token)?;
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
            all_sources.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

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

            show_phase(&mut on_chunk, &phase2);
            hold_phase(Duration::from_millis(1200), cancel_token)?;

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
            show_phase(&mut on_chunk, &phase3);
            hold_phase(Duration::from_millis(600), cancel_token)?;
        }
    }

    show_phase(&mut on_chunk, &full_content);
    Ok(full_content)
}

/// Each progress phase replaces the previous one instead of appending to it.
fn show_phase(on_chunk: &mut impl FnMut(&str), phase: &str) {
    on_chunk(&format!("{}{phase}", crate::api::WIPE_SIGNAL));
}

/// Keeps a progress phase readable briefly; Stop ends the wait at once.
fn hold_phase(duration: Duration, cancel_token: &Option<Arc<AtomicBool>>) -> Result<()> {
    if !crate::api::groq::wait_unless_cancelled(duration, cancel_token) {
        anyhow::bail!("Groq compound request cancelled");
    }
    Ok(())
}

#[cfg(test)]
#[path = "compound_tests.rs"]
mod tests;
