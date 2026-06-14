//! Shared helper for OpenAI-compatible `/chat/completions` providers.
//!
//! Several providers (Cerebras, OpenRouter, ...) hit the same OpenAI-style
//! endpoint with the same POST + SSE streaming loop and the same non-streaming
//! parse. This module centralizes that core so the per-provider functions stay
//! thin wrappers that only build their payload + provider-specific preamble.

use crate::api::client::UREQ_AGENT;
use crate::api::types::{ChatCompletionResponse, StreamChunk};
use crate::gui::locale::LocaleText;
use anyhow::Result;
use std::io::{BufRead, BufReader};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use ureq::http::HeaderMap;

/// POST to an OpenAI-compatible `/chat/completions` endpoint and stream (or
/// parse) the response, invoking `on_chunk` for each piece of content.
///
/// * `endpoint` — full chat-completions URL.
/// * `api_key` — bearer token sent as `Authorization: Bearer <key>`.
/// * `model` — model id.
/// * `messages` — the `messages` array value (callers build plain-text or
///   multimodal content as needed).
/// * `streaming` — request + consume an SSE stream when `true`.
/// * `reasoning_fallback` — when `true`, treat a content-less leading chunk as
///   "thinking" even without an explicit `reasoning` delta (Cerebras
///   gpt-oss/zai-glm behavior). OpenRouter passes `false`.
/// * `ui_language` — locale for the "thinking" indicator string.
/// * `cancel_token` — cooperative cancellation flag.
/// * `error_label` — prefix used in non-auth error messages.
/// * `map_auth_errors` — when `true`, map HTTP 401/403 to `INVALID_API_KEY`.
/// * `on_headers` — invoked with the response headers right after the POST
///   succeeds (used to record rate-limit usage).
/// * `on_chunk` — invoked with each content chunk / thinking indicator.
#[allow(clippy::too_many_arguments)]
pub fn stream_openai_compat_chat<F, H>(
    endpoint: &str,
    api_key: &str,
    model: &str,
    messages: serde_json::Value,
    streaming: bool,
    reasoning_fallback: bool,
    ui_language: &str,
    cancel_token: &Option<Arc<AtomicBool>>,
    error_label: &str,
    map_auth_errors: bool,
    on_headers: H,
    on_chunk: &mut F,
) -> Result<String>
where
    F: FnMut(&str),
    H: FnOnce(&HeaderMap),
{
    let payload = serde_json::json!({
        "model": model,
        "messages": messages,
        "stream": streaming
    });

    let resp = UREQ_AGENT
        .post(endpoint)
        .header("Authorization", &format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .send_json(payload)
        .map_err(|e| {
            let err_str = e.to_string();
            if map_auth_errors && (err_str.contains("401") || err_str.contains("403")) {
                anyhow::anyhow!("INVALID_API_KEY")
            } else {
                anyhow::anyhow!("{}: {}", error_label, err_str)
            }
        })?;

    on_headers(resp.headers());

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
                        } else if reasoning_fallback && !content_started && !thinking_shown {
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
