//! Shared helper for the Gemini (Google) `generateContent` REST path.
//!
//! The translate / refine / vision flows all build the same URL, inject the
//! same thinking config + grounding tools, POST with `x-goog-api-key`, and run
//! the same thinking-aware SSE loop (plus the same thought-filtered
//! non-streaming parse). This module centralizes that so callers only have to
//! supply the `parts` array (text, text+image, ...) and their error labeling.

use crate::api::client::UREQ_AGENT;
use crate::gui::locale::LocaleText;
use anyhow::Result;
use std::io::{BufRead, BufReader};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// Build the `generateContent` URL for `model`, selecting the SSE streaming
/// variant when `streaming` is `true`.
pub fn gemini_content_url(model: &str, streaming: bool) -> String {
    if streaming {
        format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:streamGenerateContent?alt=sse",
            model
        )
    } else {
        format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
            model
        )
    }
}

/// POST a Gemini `generateContent` request whose user content is `parts`, then
/// stream (or parse) the thinking-aware response, invoking `on_chunk`.
///
/// * `parts` — the `contents[0].parts` array (text / text+image / text+audio).
/// * `model` — model id.
/// * `api_key` — sent as `x-goog-api-key`.
/// * `streaming` — request + consume an SSE stream when `true`.
/// * `ui_language` — locale for the "thinking" indicator string.
/// * `cancel_token` — cooperative cancellation flag.
/// * `error_label` — when `Some(label)`, non-auth errors become
///   `"{label}: {err}"`; when `None`, the raw error string is used.
/// * `map_auth_errors` — when `true`, map HTTP 401/403 to `INVALID_API_KEY`.
/// * `on_chunk` — invoked with each content chunk / thinking indicator.
#[allow(clippy::too_many_arguments)]
pub fn stream_gemini_generate<F>(
    parts: serde_json::Value,
    model: &str,
    api_key: &str,
    streaming: bool,
    ui_language: &str,
    cancel_token: &Option<Arc<AtomicBool>>,
    error_label: Option<&str>,
    map_auth_errors: bool,
    on_chunk: &mut F,
) -> Result<String>
where
    F: FnMut(&str),
{
    let url = gemini_content_url(model, streaming);

    let mut payload = serde_json::json!({
        "contents": [{
            "role": "user",
            "parts": parts
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
        .header("x-goog-api-key", api_key)
        .send_json(payload)
        .map_err(|e| {
            let err_str = e.to_string();
            if map_auth_errors && (err_str.contains("401") || err_str.contains("403")) {
                anyhow::anyhow!("INVALID_API_KEY")
            } else if let Some(label) = error_label {
                anyhow::anyhow!("{}: {}", label, err_str)
            } else {
                anyhow::anyhow!("{}", err_str)
            }
        })?;

    let mut full_content = String::new();

    if streaming {
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
