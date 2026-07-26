use crate::APP;
use std::sync::LazyLock;
use std::time::Duration;
use ureq::http::HeaderMap;

/// Build a ureq agent carrying our user-agent string and an end-to-end timeout.
fn build_agent(timeout_global: Duration, http_status_as_error: bool) -> ureq::Agent {
    ureq::Agent::config_builder()
        .user_agent(concat!(
            env!("CARGO_PKG_NAME"),
            "/",
            env!("CARGO_PKG_VERSION")
        ))
        .timeout_global(Some(timeout_global))
        .http_status_as_error(http_status_as_error)
        .build()
        .into()
}

/// Apply a tighter end-to-end budget to one request without changing the
/// shared agent's defaults for unrelated long-running work.
pub fn with_request_timeout<B>(
    request: ureq::RequestBuilder<B>,
    timeout: Option<Duration>,
) -> ureq::RequestBuilder<B> {
    match timeout {
        Some(timeout) => request.config().timeout_global(Some(timeout)).build(),
        None => request,
    }
}

/// Agent for unary (non-streaming) requests — bounded end-to-end at 120s.
pub static UREQ_AGENT: LazyLock<ureq::Agent> =
    LazyLock::new(|| build_agent(Duration::from_secs(120), true));

/// Unary agent that returns HTTP error responses so callers can inspect provider
/// retry headers and structured error bodies before deciding how to recover.
pub static UREQ_RESPONSE_AGENT: LazyLock<ureq::Agent> =
    LazyLock::new(|| build_agent(Duration::from_secs(120), false));

/// Agent for streaming (SSE) requests. In ureq 3.x `timeout_global` includes body
/// reads, so a reasoning / search-grounded LLM stream that legitimately runs past
/// 120s was being force-aborted mid-response on the shared agent. Streaming calls
/// use this longer cap (matching the help-assistant agent) instead.
pub static UREQ_STREAM_AGENT: LazyLock<ureq::Agent> =
    LazyLock::new(|| build_agent(Duration::from_secs(900), true));

/// Streaming agent that preserves HTTP error responses for bounded provider
/// diagnostics while retaining the long-lived SSE timeout.
pub static UREQ_STREAM_RESPONSE_AGENT: LazyLock<ureq::Agent> =
    LazyLock::new(|| build_agent(Duration::from_secs(900), false));

/// True when a ureq error is an HTTP 401/403 (authentication failure).
///
/// In ureq 3.x an error status surfaces as the typed `Error::StatusCode`, so this
/// matches the code directly instead of substring-scanning the Display text — which
/// false-positives on a transport error whose URL or body happens to contain
/// "401"/"403" and can wrongly flag a provider's key as invalid (and permanently
/// block it). Also covers 403, which several call sites previously missed.
pub fn is_auth_error(e: &ureq::Error) -> bool {
    matches!(e, ureq::Error::StatusCode(401 | 403))
}

/// Capture the provider's latest typed rate-limit snapshot for one API endpoint.
///
/// Call this before converting an exposed HTTP error response into an error so
/// useful 429 headers are retained. Providers with shared quota scopes are
/// normalized by `usage_key_for_response`.
pub fn record_usage_headers(provider: &str, full_name: &str, headers: &HeaderMap) {
    let Some(snapshot) = crate::usage_stats::snapshot_from_headers(
        provider,
        headers,
        crate::usage_stats::now_unix_seconds(),
    ) else {
        return;
    };
    let key = crate::usage_stats::usage_key_for_response(provider, full_name);
    if let Ok(mut app) = APP.lock() {
        app.model_usage_stats.insert(key, snapshot);
    }
}

pub fn record_usage_simple(headers: &HeaderMap, stats_key: &str) {
    record_usage_headers("groq", stats_key, headers);
}

/// Log Groq's automatic prompt-cache contribution without changing quota UI.
/// Cache hits are response metadata; no request flag enables them.
pub fn record_groq_json_usage(stats_key: &str, root: &serde_json::Value) {
    let Some(usage) = root.get("usage") else {
        return;
    };
    let prompt_tokens = usage
        .get("prompt_tokens")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let cached_tokens = usage
        .pointer("/prompt_tokens_details/cached_tokens")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    if prompt_tokens > 0 {
        crate::log_info!(
            "[Groq][cache] model={} cached_tokens={}/{} ({:.1}%)",
            stats_key,
            cached_tokens,
            prompt_tokens,
            cached_tokens as f64 * 100.0 / prompt_tokens as f64
        );
    }
}

/// Record model usage from the Cerebras rate-limit headers.
///
/// Prefers the per-day headers (`-requests-day`) and falls back to the
/// non-day variants. When the limit is still unknown, falls back to the
/// model catalog's `quota_limit_en` value. The store is updated whenever
/// either remaining or limit is known.
pub fn record_usage_cerebras(headers: &HeaderMap, stats_key: &str) {
    record_usage_headers("cerebras", stats_key, headers);
}

/// Log Cerebras automatic prompt-cache and predicted-output contribution.
pub fn record_cerebras_json_usage(stats_key: &str, root: &serde_json::Value) {
    let cached = root
        .pointer("/usage/prompt_tokens_details/cached_tokens")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let accepted = root
        .pointer("/usage/completion_tokens_details/accepted_prediction_tokens")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let rejected = root
        .pointer("/usage/completion_tokens_details/rejected_prediction_tokens")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    if cached > 0 || accepted > 0 || rejected > 0 {
        crate::log_info!(
            "[Cerebras][usage] model={} cached_tokens={} prediction_accepted={} prediction_rejected={}",
            stats_key,
            cached,
            accepted,
            rejected
        );
    }
}
