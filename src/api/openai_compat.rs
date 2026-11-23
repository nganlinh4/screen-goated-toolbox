//! Shared helper for OpenAI-compatible `/chat/completions` providers.
//!
//! Several providers (Groq, OpenRouter, ...) hit the same OpenAI-style
//! endpoint with the same POST + SSE streaming loop and the same non-streaming
//! parse. This module centralizes that core so the per-provider functions stay
//! thin wrappers that only build their payload + provider-specific preamble.

use crate::api::client::{UREQ_RESPONSE_AGENT, UREQ_STREAM_RESPONSE_AGENT};
use crate::gui::locale::LocaleText;
use anyhow::Result;
use std::io::{BufRead, BufReader, Read};
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
/// * `payload` — the provider-native request body.
/// * `streaming` — request + consume an SSE stream when `true`.
/// * `reasoning_fallback` — when `true`, treat a content-less leading chunk as
///   "thinking" even without an explicit `reasoning` delta.
/// * `ui_language` — locale for the "thinking" indicator string.
/// * `cancel_token` — cooperative cancellation flag.
/// * `error_label` — prefix used in non-auth error messages.
/// * `credential_provider` — when set, HTTP 401/403 become
///   `INVALID_API_KEY:<provider>` so the notice names the key to fix.
/// * `on_headers` — invoked with the response headers right after the POST
///   succeeds (used to record rate-limit usage).
/// * `on_chunk` — invoked with each content chunk / thinking indicator.
#[allow(clippy::too_many_arguments)]
pub fn stream_openai_compat_payload<F, H>(
    endpoint: &str,
    api_key: &str,
    payload: serde_json::Value,
    streaming: bool,
    reasoning_fallback: bool,
    ui_language: &str,
    cancel_token: &Option<Arc<AtomicBool>>,
    request_timeout: Option<crate::api::client::RequestTimeouts>,
    error_label: &str,
    credential_provider: Option<&str>,
    on_headers: H,
    on_chunk: &mut F,
) -> Result<String>
where
    F: FnMut(&str),
    H: FnOnce(&HeaderMap),
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
    let resp = request.send(serde_json::to_vec(&payload)?).map_err(|e| {
        anyhow::anyhow!(crate::api::client::transport_error_message(error_label, &e))
    })?;

    on_headers(resp.headers());
    let status = resp.status().as_u16();
    if !resp.status().is_success() {
        if let Some(error) = credential_error(credential_provider, status) {
            return Err(error);
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
        full_content = parse_chat_completion(&root)?;
        on_chunk(&full_content);
    }

    Ok(full_content)
}

fn credential_error(provider: Option<&str>, status: u16) -> Option<anyhow::Error> {
    let provider = provider?;
    matches!(status, 401 | 403).then(|| anyhow::anyhow!("INVALID_API_KEY:{provider}"))
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
/// reasoning/thinking-aware variant is built into [`stream_openai_compat_payload`].
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
    use super::{credential_error, provider_error_message};

    #[test]
    fn credential_errors_name_the_provider_whose_key_failed() {
        for status in [401, 403] {
            let error = credential_error(Some("nvidia"), status)
                .unwrap()
                .to_string();
            assert_eq!(error, "INVALID_API_KEY:nvidia");
            let message = crate::credential_error::message(&error, "en").unwrap();
            assert!(!message.contains("API API"), "{message}");
            assert_ne!(crate::credential_error::provider_name(&error), "API");
        }
        assert!(credential_error(Some("openrouter"), 429).is_none());
        assert!(credential_error(None, 401).is_none());
    }

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
