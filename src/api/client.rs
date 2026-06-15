use crate::APP;
use std::sync::LazyLock;
use std::time::Duration;
use ureq::http::HeaderMap;

/// Build a ureq agent carrying our user-agent string and an end-to-end timeout.
fn build_agent(timeout_global: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .user_agent(concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")))
        .timeout_global(Some(timeout_global))
        .build()
        .into()
}

/// Agent for unary (non-streaming) requests — bounded end-to-end at 120s.
pub static UREQ_AGENT: LazyLock<ureq::Agent> =
    LazyLock::new(|| build_agent(Duration::from_secs(120)));

/// Agent for streaming (SSE) requests. In ureq 3.x `timeout_global` includes body
/// reads, so a reasoning / search-grounded LLM stream that legitimately runs past
/// 120s was being force-aborted mid-response on the shared agent. Streaming calls
/// use this longer cap (matching the help-assistant agent) instead.
pub static UREQ_STREAM_AGENT: LazyLock<ureq::Agent> =
    LazyLock::new(|| build_agent(Duration::from_secs(900)));

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

/// Read a response header as a `&str`, if present and valid UTF-8.
fn header_str<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|v| v.to_str().ok())
}

/// Store a `remaining / limit` usage string for `stats_key` in the shared app state.
fn store_usage(stats_key: &str, usage_str: String) {
    if let Ok(mut app) = APP.lock() {
        app.model_usage_stats.insert(stats_key.to_string(), usage_str);
    }
}

/// Record model usage from the common Groq/Whisper rate-limit headers.
///
/// Reads `x-ratelimit-remaining-requests` and (when present)
/// `x-ratelimit-limit-requests`. Only updates the store when the remaining
/// header is present; a missing limit header falls back to `"?"`.
pub fn record_usage_simple(headers: &HeaderMap, stats_key: &str) {
    if let Some(remaining) = header_str(headers, "x-ratelimit-remaining-requests") {
        let limit = header_str(headers, "x-ratelimit-limit-requests").unwrap_or("?");
        store_usage(stats_key, format!("{} / {}", remaining, limit));
    }
}

/// Record model usage from the Cerebras rate-limit headers.
///
/// Prefers the per-day headers (`-requests-day`) and falls back to the
/// non-day variants. When the limit is still unknown, falls back to the
/// model catalog's `quota_limit_en` value. The store is updated whenever
/// either remaining or limit is known.
pub fn record_usage_cerebras(headers: &HeaderMap, stats_key: &str) {
    let remaining = header_str(headers, "x-ratelimit-remaining-requests-day")
        .or_else(|| header_str(headers, "x-ratelimit-remaining-requests"))
        .unwrap_or("?");

    let mut limit = header_str(headers, "x-ratelimit-limit-requests-day")
        .or_else(|| header_str(headers, "x-ratelimit-limit-requests"))
        .unwrap_or("?")
        .to_string();

    if limit == "?"
        && let Some(conf) = crate::model_config::get_model_by_id(stats_key)
        && let Some(val) = conf.quota_limit_en.split_whitespace().next()
    {
        limit = val.to_string();
    }

    if remaining != "?" || limit != "?" {
        store_usage(stats_key, format!("{} / {}", remaining, limit));
    }
}

/// Record model usage from the realtime-audio token rate-limit headers.
///
/// Reads `x-ratelimit-remaining-requests-tokens` and (when present)
/// `x-ratelimit-limit-tokens`. Only updates the store when the remaining
/// header is present; a missing limit header falls back to `"?"`.
pub fn record_usage_tokens(headers: &HeaderMap, stats_key: &str) {
    if let Some(remaining) = header_str(headers, "x-ratelimit-remaining-requests-tokens") {
        let limit = header_str(headers, "x-ratelimit-limit-tokens").unwrap_or("?");
        store_usage(stats_key, format!("{} / {}", remaining, limit));
    }
}
