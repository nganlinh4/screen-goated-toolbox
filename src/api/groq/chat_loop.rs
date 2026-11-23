//! The Groq chat retry contract, independent of how one request is sent, so
//! the host and the recorder worker share it while keeping their own agents.

use std::sync::{Arc, atomic::AtomicBool};
use std::time::Duration;

use anyhow::Result;

pub const CHAT_COMPLETIONS_URL: &str = "https://api.groq.com/openai/v1/chat/completions";

pub(crate) type ChatResponse = ureq::http::Response<ureq::Body>;

/// Sends `payload` through `transmit`, adopting a provider-declared output
/// allowance once and, when `retry_short_rate_limit` is set, waiting out one
/// short 429 delay in place. `on_retry` observes each retry with the time it
/// waited. Failures use the shared Groq error mapping.
pub(crate) fn send_with_retries(
    payload: &mut serde_json::Value,
    cancel_token: &Option<Arc<AtomicBool>>,
    retry_short_rate_limit: bool,
    mut transmit: impl FnMut(&serde_json::Value) -> Result<ChatResponse>,
    mut on_retry: impl FnMut(Duration),
) -> Result<ChatResponse> {
    let model = payload["model"].as_str().unwrap_or_default().to_owned();
    let mut attempt = 0u8;
    loop {
        let response = transmit(payload)?;
        crate::api::client::record_usage_simple(response.headers(), &model);
        if response.status().is_success() {
            return Ok(response);
        }
        let status = response.status().as_u16();
        let retry_after = super::retry_after_seconds(response.headers());
        let body = response.into_body().read_to_string().unwrap_or_default();
        if let Some(limit) = super::output_limit::output_limit_retry(
            status,
            attempt,
            &body,
            payload["max_completion_tokens"].as_u64(),
        ) {
            payload["max_completion_tokens"] = limit.into();
            crate::log_info!("[Groq] retrying with provider-declared output allowance={limit}");
            on_retry(Duration::ZERO);
            attempt += 1;
            continue;
        }
        if retry_short_rate_limit
            && let Some(seconds) = super::groq_rate_limit_retry_delay(status, attempt, retry_after)
        {
            crate::log_info!("[Groq] token limit reached; retrying once in {seconds}s");
            on_retry(Duration::from_secs(seconds));
            if !super::wait_for_groq_retry(seconds, cancel_token) {
                anyhow::bail!("Groq request cancelled");
            }
            attempt += 1;
            continue;
        }
        return Err(super::http_error(status, &body));
    }
}
