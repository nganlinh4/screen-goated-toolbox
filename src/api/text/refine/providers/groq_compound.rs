use crate::api::client::{UREQ_AGENT, record_groq_json_usage, record_usage_simple};
use crate::gui::locale::LocaleText;
use crate::overlay::utils::get_context_quote;
use anyhow::Result;

// --- GROQ COMPOUND REFINE ---
pub(super) fn refine_groq_compound<F>(
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

    record_usage_simple(resp.headers(), p_model);

    let json: serde_json::Value = resp.into_body().read_json()?;
    record_groq_json_usage(p_model, &json);
    let mut full_content = String::new();

    if let Some(choices) = json.get("choices").and_then(|c| c.as_array())
        && let Some(first_choice) = choices.first()
        && let Some(message) = first_choice.get("message")
    {
        if let Some(executed_tools) = message.get("executed_tools").and_then(|t| t.as_array()) {
            report_search_progress(executed_tools, final_prompt, &locale, on_chunk);
        }

        if let Some(content) = message.get("content").and_then(|c| c.as_str()) {
            full_content = content.to_string();
            on_chunk(&full_content);
        }
    }

    Ok(full_content)
}

fn report_search_progress<F>(
    executed_tools: &[serde_json::Value],
    final_prompt: &str,
    locale: &LocaleText,
    on_chunk: &mut F,
) where
    F: FnMut(&str),
{
    let search_queries = collect_search_queries(executed_tools);
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

    let mut all_sources = collect_search_sources(executed_tools, locale);
    if !all_sources.is_empty() {
        all_sources.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
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

fn collect_search_queries(executed_tools: &[serde_json::Value]) -> Vec<String> {
    let mut search_queries = Vec::new();
    for tool in executed_tools {
        if tool.get("type").and_then(|t| t.as_str()) == Some("search")
            && let Some(args) = tool.get("arguments").and_then(|a| a.as_str())
            && let Ok(args_json) = serde_json::from_str::<serde_json::Value>(args)
            && let Some(query) = args_json.get("query").and_then(|q| q.as_str())
        {
            search_queries.push(query.to_string());
        }
    }
    search_queries
}

fn collect_search_sources(
    executed_tools: &[serde_json::Value],
    locale: &LocaleText,
) -> Vec<(String, String, f64)> {
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
    all_sources
}
