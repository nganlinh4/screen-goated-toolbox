//! Browser-backed research helper.
//!
//! This tool gives the harness one compact, source-aware path for web
//! verification instead of relying on the live model to sequence search, source
//! selection, page reads, and provenance itself.

use serde_json::{Value, json};

const MAX_PAGE_CHARS: usize = 5000;

pub(super) fn research_web(args: &Value) -> Value {
    let query = args
        .get("query")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if query.is_empty() {
        return json!({
            "ok": false,
            "code": "ERR_RESEARCH_QUERY_REQUIRED",
            "error": "research_web needs a non-empty query",
        });
    }
    let policy = args
        .get("source_policy")
        .and_then(Value::as_str)
        .unwrap_or("best_available");
    let max_sources = args
        .get("max_sources")
        .and_then(Value::as_u64)
        .unwrap_or(3)
        .clamp(1, 5) as usize;

    super::telemetry::event(
        "research_start",
        "research",
        super::telemetry::Privacy::Safe,
        json!({
            "query_char_count": query.chars().count(),
            "query_byte_count": query.len(),
            "source_policy": policy,
            "max_sources": max_sources,
        }),
    );

    if !super::browser::is_connected() {
        return json!({
            "ok": false,
            "code": "ERR_RESEARCH_BROWSER_NOT_CONNECTED",
            "error": "deep browser control is not connected",
            "instruction": "Call browser_status/browser_setup, or answer that web verification cannot run until browser control is connected.",
        });
    }

    let search_url = format!(
        "https://www.google.com/search?q={}",
        urlencoding::encode(query)
    );
    let search_tab = match TemporaryTab::open(&search_url) {
        Ok(tab) => tab,
        Err(error) => return browser_error(error),
    };
    std::thread::sleep(std::time::Duration::from_millis(900));
    let search_page = super::browser::read_page_on_tab(search_tab.id);
    if search_page.get("ok").and_then(Value::as_bool) != Some(true) {
        return search_page;
    }

    let links = search_links(search_tab.id, max_sources);
    let mut sources = Vec::new();
    for link in links.iter().take(max_sources) {
        let tab = match TemporaryTab::open(link) {
            Ok(tab) => tab,
            Err(_) => continue,
        };
        std::thread::sleep(std::time::Duration::from_millis(900));
        let page = super::browser::read_page_on_tab(tab.id);
        if page.get("ok").and_then(Value::as_bool) != Some(true) {
            continue;
        }
        sources.push(source_from_page(&page));
    }

    if sources.is_empty() {
        sources.push(source_from_page(&search_page));
    }
    let confidence = if sources.len() >= 2 { "medium" } else { "low" };
    let answer_material = sources
        .iter()
        .filter_map(|s| s.get("text_preview").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n\n---\n\n");

    super::telemetry::event(
        "research_complete",
        "research",
        super::telemetry::Privacy::Safe,
        json!({
            "query_char_count": query.chars().count(),
            "query_byte_count": query.len(),
            "source_count": sources.len(),
            "confidence": confidence,
        }),
    );

    json!({
        "ok": true,
        "query": query,
        "source_policy": policy,
        "confidence": confidence,
        "sources": sources,
        "answer_material": answer_material.chars().take(9000).collect::<String>(),
        "instruction": "Use answer_material and cite/source-name the sources in your spoken answer. If sources disagree or are only search snippets, say that clearly.",
    })
}

fn search_links(tab_id: i64, max_sources: usize) -> Vec<String> {
    let js = r#"(() => Array.from(document.querySelectorAll('a'))
      .map(a => a.href || '')
      .filter(h => /^https?:\/\//.test(h))
      .filter(h => !h.includes('google.') && !h.includes('/search?') && !h.includes('/preferences'))
      .slice(0, 12))()"#;
    match super::browser::eval_value_on_tab(js, tab_id) {
        Ok(Value::Array(arr)) => arr
            .iter()
            .filter_map(Value::as_str)
            .map(clean_google_url)
            .filter(|u| !u.is_empty())
            .take(max_sources)
            .collect(),
        _ => Vec::new(),
    }
}

struct TemporaryTab {
    id: i64,
}

impl TemporaryTab {
    fn open(url: &str) -> anyhow::Result<Self> {
        let tab = super::browser::open_temporary_tab(url)?;
        super::telemetry::event(
            "research_surface_opened",
            "research",
            super::telemetry::Privacy::Safe,
            json!({"tab_id": tab.id, "foreground": tab.foreground}),
        );
        Ok(Self { id: tab.id })
    }
}

impl Drop for TemporaryTab {
    fn drop(&mut self) {
        let result = super::browser::close_tab(self.id);
        super::telemetry::event(
            "research_surface_closed",
            "research",
            super::telemetry::Privacy::Safe,
            json!({"tab_id": self.id, "ok": result.is_ok()}),
        );
    }
}

fn browser_error(error: anyhow::Error) -> Value {
    let typed = super::browser::err(error);
    if typed.get("code").and_then(Value::as_str) == Some("ERR_BROWSER_CAPABILITY_UNSUPPORTED") {
        return typed;
    }
    json!({
        "ok": false,
        "code": "ERR_RESEARCH_BROWSER_TOOL_FAILED",
        "error": typed.get("error").cloned().unwrap_or(Value::Null),
    })
}

fn clean_google_url(url: &str) -> String {
    if let Ok(parsed) = url::Url::parse(url)
        && parsed.domain().is_some_and(|d| d.contains("google."))
        && parsed.path().contains("/url")
    {
        for (key, value) in parsed.query_pairs() {
            if key == "q" || key == "url" {
                return value.to_string();
            }
        }
    }
    url.to_string()
}

fn source_from_page(page: &Value) -> Value {
    let p = page.get("page").unwrap_or(page);
    let title = p.get("title").and_then(Value::as_str).unwrap_or("");
    let url = p.get("url").and_then(Value::as_str).unwrap_or("");
    let text = p.get("text").and_then(Value::as_str).unwrap_or("");
    let artifact = page.get("artifact").cloned().unwrap_or(Value::Null);
    json!({
        "title": title,
        "url": url,
        "source_kind": source_kind(url),
        "char_count": p.get("char_count").cloned().unwrap_or(Value::Null),
        "text_preview": text.chars().take(MAX_PAGE_CHARS).collect::<String>(),
        "artifact": artifact,
    })
}

fn source_kind(url: &str) -> &'static str {
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.domain().map(str::to_string))
        .map(|domain| {
            if domain.ends_with(".gov") || domain.ends_with(".edu") {
                "institutional"
            } else if domain.contains("docs.")
                || domain.contains("developer.")
                || domain.contains("support.")
                || domain.contains("help.")
            {
                "documentation"
            } else {
                "web"
            }
        })
        .unwrap_or("unknown")
}
