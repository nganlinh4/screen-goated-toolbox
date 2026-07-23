//! Shared helper for the Gemini (Google) `generateContent` REST path.
//!
//! The translate / refine / vision flows all build the same URL, inject the
//! same thinking config, POST with `x-goog-api-key`, and run
//! the same thinking-aware SSE loop (plus the same thought-filtered
//! non-streaming parse). This module centralizes that so callers only have to
//! supply the `parts` array (text, text+image, ...) and their error labeling.

use crate::api::client::{UREQ_RESPONSE_AGENT, UREQ_STREAM_RESPONSE_AGENT};
use crate::gui::locale::LocaleText;
use anyhow::Result;
use std::io::{BufRead, BufReader, Read};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const MAX_GEMINI_ERROR_BODY_BYTES: u64 = 8 * 1024;
const MAX_GEMINI_ERROR_CHARS: usize = 2_048;
const MAX_GEMINI_RETRIES: usize = 2;
const MAX_GEMINI_RETRY_DELAY: Duration = Duration::from_secs(8);

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
/// * `response_schema` — when `Some` and the model is Gemma 4, sent as
///   `responseJsonSchema` (Gemma needs a full schema to emit JSON reliably;
///   ignored for other models, which comply from the prompt).
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
    request_timeout: Option<Duration>,
    response_schema: Option<&serde_json::Value>,
    on_chunk: &mut F,
) -> Result<String>
where
    F: FnMut(&str),
{
    let url = gemini_content_url(model, streaming);

    let payload = gemini_payload(parts, model, response_schema);

    // Optional provider tools are deliberately absent. Supporting a tool is a
    // capability, not consent to invoke its separate quota/cost on every request.
    // A caller that needs grounding must use an explicit tool-aware request path.
    let agent = if streaming {
        &*UREQ_STREAM_RESPONSE_AGENT
    } else {
        &*UREQ_RESPONSE_AGENT
    };
    let mut retry_attempt = 0;
    let resp = loop {
        let request = agent.post(&url).header("x-goog-api-key", api_key);
        let response = crate::api::client::with_request_timeout(request, request_timeout)
            .send_json(&payload)
            .map_err(|error| labeled_error(error_label, format!("transport error: {error}")))?;
        let status = response.status().as_u16();
        if response.status().is_success() {
            break response;
        }
        if map_auth_errors && matches!(status, 401 | 403) {
            return Err(anyhow::anyhow!("INVALID_API_KEY"));
        }

        let header_delay = retry_after_seconds(response.headers());
        let body = read_error_body(response);
        let error = GeminiHttpError::parse(status, &body, header_delay);
        if error.retryable()
            && retry_attempt < MAX_GEMINI_RETRIES
            && error
                .retry_after
                .is_none_or(|delay| delay <= MAX_GEMINI_RETRY_DELAY)
        {
            let delay = gemini_retry_delay(retry_attempt, error.retry_after);
            crate::log_info!(
                "[Gemini] transient HTTP {} for model={}; retry {}/{} in {}ms",
                status,
                model,
                retry_attempt + 1,
                MAX_GEMINI_RETRIES,
                delay.as_millis()
            );
            if !wait_for_retry(delay, cancel_token) {
                return Err(anyhow::anyhow!("Cancelled"));
            }
            retry_attempt += 1;
            continue;
        }
        return Err(labeled_error(error_label, error.display()));
    };

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
                                    on_chunk(locale.global_settings.model_thinking);
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

fn gemini_payload(
    parts: serde_json::Value,
    model: &str,
    response_schema: Option<&serde_json::Value>,
) -> serde_json::Value {
    let mut payload = serde_json::json!({
        "contents": [{
            "role": "user",
            "parts": parts
        }]
    });
    let mut gen_config = serde_json::Map::new();
    if let Some(thinking_config) = crate::api::gemini_thinking_config(model) {
        gen_config.insert("thinkingConfig".to_string(), thinking_config);
    }
    // Gemma 4 ignores `responseMimeType` alone and free-rambles; it only emits
    // reliable structured JSON when handed a full `responseJsonSchema`. Other Gemini
    // models comply straight from the prompt, so attach it for Gemma only (forcing a
    // schema on the others can derail their tuned output).
    if let Some(schema) = response_schema
        && model.contains("gemma-4")
    {
        gen_config.insert(
            "responseMimeType".to_string(),
            serde_json::json!("application/json"),
        );
        gen_config.insert("responseJsonSchema".to_string(), schema.clone());
    }
    if !gen_config.is_empty() {
        payload["generationConfig"] = serde_json::Value::Object(gen_config);
    }
    payload
}

#[derive(Debug)]
struct GeminiHttpError {
    status: u16,
    api_status: Option<String>,
    message: String,
    details: Option<String>,
    retry_after: Option<Duration>,
}

impl GeminiHttpError {
    fn parse(status: u16, body: &str, header_delay: Option<Duration>) -> Self {
        let root = serde_json::from_str::<serde_json::Value>(body).ok();
        let error = root.as_ref().and_then(|value| value.get("error"));
        let api_status = error
            .and_then(|value| value.get("status"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        let message = error
            .and_then(|value| value.get("message"))
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| {
                if body.trim().is_empty() {
                    "empty provider error"
                } else {
                    body
                }
            })
            .to_string();
        let details_value = error.and_then(|value| value.get("details"));
        let retry_after = header_delay.or_else(|| retry_delay_from_details(details_value));
        let details = details_value
            .filter(|value| !value.is_null())
            .and_then(|value| serde_json::to_string(value).ok());
        Self {
            status,
            api_status,
            message,
            details,
            retry_after,
        }
    }

    fn retryable(&self) -> bool {
        matches!(self.status, 429 | 500 | 502 | 503 | 504)
    }

    fn display(&self) -> String {
        let mut output = format!("Gemini API HTTP {}", self.status);
        if let Some(status) = &self.api_status {
            output.push(' ');
            output.push_str(status);
        }
        output.push_str(": ");
        output.push_str(&self.message);
        if let Some(delay) = self.retry_after {
            output.push_str(&format!("; retry_after_ms={}", delay.as_millis()));
        }
        if let Some(details) = &self.details {
            output.push_str("; details=");
            output.push_str(details);
        }
        truncate_chars(output, MAX_GEMINI_ERROR_CHARS)
    }
}

fn labeled_error(label: Option<&str>, message: String) -> anyhow::Error {
    match label {
        Some(label) => anyhow::anyhow!("{label}: {message}"),
        None => anyhow::anyhow!(message),
    }
}

fn read_error_body(response: ureq::http::Response<ureq::Body>) -> String {
    let mut body = String::new();
    let _ = response
        .into_body()
        .into_reader()
        .take(MAX_GEMINI_ERROR_BODY_BYTES)
        .read_to_string(&mut body);
    body
}

fn retry_after_seconds(headers: &ureq::http::HeaderMap) -> Option<Duration> {
    let seconds = headers
        .get("retry-after")?
        .to_str()
        .ok()?
        .parse::<f64>()
        .ok()?;
    (seconds.is_finite() && seconds >= 0.0).then(|| Duration::from_secs_f64(seconds))
}

fn retry_delay_from_details(details: Option<&serde_json::Value>) -> Option<Duration> {
    let values = details?.as_array()?;
    values.iter().find_map(|detail| {
        let seconds = detail
            .get("retryDelay")?
            .as_str()?
            .strip_suffix('s')?
            .parse::<f64>()
            .ok()?;
        (seconds.is_finite() && seconds >= 0.0).then(|| Duration::from_secs_f64(seconds))
    })
}

fn gemini_retry_delay(attempt: usize, requested: Option<Duration>) -> Duration {
    if let Some(delay) = requested {
        return delay.min(MAX_GEMINI_RETRY_DELAY);
    }
    let base_ms = 1_000_u64.saturating_mul(1_u64 << attempt.min(3));
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.subsec_nanos() as u64)
        .unwrap_or(0);
    let jitter_ms = nanos % (base_ms / 2 + 1);
    Duration::from_millis((base_ms + jitter_ms).min(MAX_GEMINI_RETRY_DELAY.as_millis() as u64))
}

fn wait_for_retry(delay: Duration, cancel_token: &Option<Arc<AtomicBool>>) -> bool {
    let started = std::time::Instant::now();
    while started.elapsed() < delay {
        if cancel_token
            .as_ref()
            .is_some_and(|token| token.load(Ordering::Relaxed))
        {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50).min(delay.saturating_sub(started.elapsed())));
    }
    true
}

fn truncate_chars(value: String, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value;
    }
    value.chars().take(max_chars).collect::<String>() + "…"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_generation_never_implicitly_enables_provider_tools() {
        let payload = gemini_payload(
            serde_json::json!([{"text": "plain request"}]),
            "gemini-future-model",
            None,
        );
        assert!(payload.get("tools").is_none());
    }

    #[test]
    fn structured_error_keeps_quota_metadata_and_retry_delay() {
        let error = GeminiHttpError::parse(
            429,
            r#"{"error":{"status":"RESOURCE_EXHAUSTED","message":"quota exhausted","details":[{"@type":"type.googleapis.com/google.rpc.RetryInfo","retryDelay":"2.25s"},{"metadata":{"quotaMetric":"generate_content","model":"future-model"}}]}}"#,
            None,
        );
        let message = error.display();
        assert!(message.contains("HTTP 429 RESOURCE_EXHAUSTED"));
        assert!(message.contains("retry_after_ms=2250"));
        assert!(message.contains("quotaMetric"));
        assert!(message.contains("future-model"));
    }

    #[test]
    fn retry_policy_is_transient_and_bounded() {
        assert!(GeminiHttpError::parse(503, "", None).retryable());
        assert!(!GeminiHttpError::parse(400, "", None).retryable());
        for attempt in 0..10 {
            assert!(gemini_retry_delay(attempt, None) <= MAX_GEMINI_RETRY_DELAY);
        }
    }

    #[test]
    fn explicit_long_retry_delay_is_preserved_for_fail_fast_fallback() {
        let delay = Duration::from_secs(45);
        let error = GeminiHttpError::parse(429, "{}", Some(delay));
        assert_eq!(error.retry_after, Some(delay));
        assert!(
            error
                .retry_after
                .is_some_and(|value| value > MAX_GEMINI_RETRY_DELAY)
        );
    }
}
