//! Shared helper for OpenAI-compatible `/chat/completions` providers.
//!
//! Several providers (Groq, OpenRouter, ...) hit the same OpenAI-style
//! endpoint with the same POST + SSE streaming loop and the same non-streaming
//! parse. This module centralizes that core so the per-provider functions stay
//! thin wrappers that only build their payload + provider-specific preamble.

use crate::api::client::{UREQ_RESPONSE_AGENT, UREQ_STREAM_RESPONSE_AGENT, is_auth_error};
use crate::gui::locale::LocaleText;
use anyhow::Result;
use flate2::{Compression, write::GzEncoder};
use std::io::{BufRead, BufReader, Read, Write};
use std::sync::{Arc, atomic::AtomicBool};
use ureq::http::HeaderMap;

const MAX_PROVIDER_ERROR_BODY_BYTES: u64 = 8 * 1024;
const MAX_PROVIDER_ERROR_MESSAGE_CHARS: usize = 1_024;

mod completion;
pub use completion::parse_chat_completion;

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
///   "thinking" even without an explicit `reasoning` delta (some OpenAI-compatible
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
    request_timeout: Option<crate::api::client::RequestTimeouts>,
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

    stream_openai_compat_payload(
        endpoint,
        api_key,
        payload,
        streaming,
        reasoning_fallback,
        ui_language,
        cancel_token,
        request_timeout,
        error_label,
        map_auth_errors,
        false,
        on_headers,
        |_| {},
        on_chunk,
    )
}

/// Payload-aware variant used when an OpenAI-compatible provider has native
/// fields such as `max_completion_tokens`, structured output, or prediction.
/// Large payload compression is opt-in because provider support is not uniform.
#[allow(clippy::too_many_arguments)]
pub fn stream_openai_compat_payload<F, H, J>(
    endpoint: &str,
    api_key: &str,
    payload: serde_json::Value,
    streaming: bool,
    reasoning_fallback: bool,
    ui_language: &str,
    cancel_token: &Option<Arc<AtomicBool>>,
    request_timeout: Option<crate::api::client::RequestTimeouts>,
    error_label: &str,
    map_auth_errors: bool,
    gzip_large_payload: bool,
    on_headers: H,
    on_json_usage: J,
    on_chunk: &mut F,
) -> Result<String>
where
    F: FnMut(&str),
    H: FnOnce(&HeaderMap),
    J: FnOnce(&serde_json::Value),
{
    let _deadline = crate::api::client::first_token::FirstTokenGuard::new(
        streaming,
        request_timeout,
        cancel_token,
    );
    // Streaming responses use a first-output deadline without
    // a whole-response cap; unary calls use their workload-derived hard budget.
    let agent = if streaming {
        &*UREQ_STREAM_RESPONSE_AGENT
    } else {
        &*UREQ_RESPONSE_AGENT
    };
    let request = agent
        .post(endpoint)
        .header("Authorization", &format!("Bearer {}", api_key))
        .header("Content-Type", "application/json");
    let request = crate::api::client::with_request_timeouts(request, request_timeout);
    let json_bytes = serde_json::to_vec(&payload)?;
    let response = if gzip_large_payload && json_bytes.len() >= 12 * 1024 {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
        encoder.write_all(&json_bytes)?;
        request
            .header("Content-Encoding", "gzip")
            .send(encoder.finish()?)
    } else {
        request.send(json_bytes)
    };
    let resp = response.map_err(|e| {
        if map_auth_errors && is_auth_error(&e) {
            anyhow::anyhow!("INVALID_API_KEY")
        } else {
            anyhow::anyhow!(crate::api::client::transport_error_message(error_label, &e))
        }
    })?;

    on_headers(resp.headers());
    let status = resp.status().as_u16();
    if !resp.status().is_success() {
        if map_auth_errors && matches!(status, 401 | 403) {
            return Err(anyhow::anyhow!("INVALID_API_KEY"));
        }
        let mut body = String::new();
        let _ = resp
            .into_body()
            .into_reader()
            .take(MAX_PROVIDER_ERROR_BODY_BYTES)
            .read_to_string(&mut body);
        return Err(anyhow::anyhow!(
            "{} HTTP {}: {}",
            error_label,
            status,
            provider_error_message(status, &body)
        ));
    }

    let full_content;

    if streaming {
        let reader = BufReader::new(resp.into_body().into_reader());
        let mut thinking_shown = false;
        let mut content_started = false;
        let locale = LocaleText::get(ui_language);

        full_content = completion::consume_stream(reader, cancel_token, |content, reasoning| {
            if (!reasoning.is_empty() || reasoning_fallback) && !content_started && !thinking_shown
            {
                on_chunk(locale.global_settings.model_thinking);
                thinking_shown = true;
            }
            if !content.is_empty() {
                if !content_started && thinking_shown {
                    on_chunk(&format!("{}{}", crate::api::WIPE_SIGNAL, content));
                } else {
                    on_chunk(content);
                }
                content_started = true;
            }
        })?;
    } else {
        let root: serde_json::Value = resp.into_body().read_json().map_err(|e| {
            anyhow::anyhow!(
                "PROVIDER_RESPONSE_INVALID:Malformed provider completion: {}",
                e
            )
        })?;
        on_json_usage(&root);
        full_content = parse_chat_completion(&root)?;
        on_chunk(&full_content);
    }

    Ok(full_content)
}

fn provider_error_message(status: u16, body: &str) -> String {
    let parsed = serde_json::from_str::<serde_json::Value>(body).ok();
    let message = parsed
        .as_ref()
        .and_then(|root| {
            ["/error/message", "/message", "/detail", "/error"]
                .into_iter()
                .find_map(|pointer| root.pointer(pointer).and_then(serde_json::Value::as_str))
        })
        .filter(|message| !message.trim().is_empty())
        .unwrap_or_else(|| {
            let body = body.trim();
            if body.is_empty() {
                "request failed"
            } else {
                body
            }
        });
    let normalized = message.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = normalized.chars();
    let mut bounded = chars
        .by_ref()
        .take(MAX_PROVIDER_ERROR_MESSAGE_CHARS)
        .collect::<String>();
    if chars.next().is_some() {
        bounded.push('…');
    }
    if bounded.is_empty() {
        format!("request failed with status {status}")
    } else {
        bounded
    }
}

/// Consume an OpenAI-compatible *streaming* chat response, appending each
/// `delta.content` chunk to a string (forwarded via `on_chunk`) until `[DONE]`,
/// honoring `cancel_token`. Returns the accumulated content. This is the simple
/// content-only loop shared by the Groq translate / vision / refine paths; the
/// reasoning/thinking-aware variant is built into [`stream_openai_compat_chat`].
pub fn consume_content_stream<R, F>(
    reader: R,
    cancel_token: &Option<Arc<AtomicBool>>,
    on_chunk: &mut F,
) -> Result<String>
where
    R: BufRead,
    F: FnMut(&str),
{
    completion::consume_stream(reader, cancel_token, |content, _| {
        if !content.is_empty() {
            on_chunk(content);
        }
    })
}

#[cfg(test)]
mod tests {
    use super::provider_error_message;

    #[test]
    fn content_stream_reports_errors_and_reasoning_only_responses() {
        let parse = |text: &str| {
            super::consume_content_stream(std::io::Cursor::new(text.as_bytes()), &None, &mut |_| {})
        };
        assert!(
            parse("data: {\"error\":{\"message\":\"completion budget exhausted\"}}\n")
                .unwrap_err()
                .to_string()
                .contains("budget exhausted")
        );
        assert!(parse("data: {\"choices\":[{\"delta\":{\"reasoning\":\"internal\"}}]}\n").is_err());
        assert!(
            parse("data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"length\"}]}\n").is_err()
        );
        assert_eq!(
            parse("data: {\"choices\":[{\"delta\":{\"content\":\"result\"}}]}\n\ndata: [DONE]\n\n")
                .unwrap(),
            "result"
        );
    }

    #[test]
    fn structured_provider_errors_expose_the_bounded_reason() {
        assert_eq!(
            provider_error_message(
                400,
                r#"{"error":{"message":"unsupported request field","type":"invalid_request"}}"#,
            ),
            "unsupported request field"
        );
    }

    #[test]
    fn plain_provider_errors_are_flattened_and_bounded() {
        assert_eq!(
            provider_error_message(413, " payload\n too\tlarge "),
            "payload too large"
        );
        assert!(
            provider_error_message(500, &"x".repeat(2_000))
                .chars()
                .count()
                <= 1_025
        );
        assert_eq!(provider_error_message(503, ""), "request failed");
    }
}
