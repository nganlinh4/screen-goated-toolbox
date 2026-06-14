use crate::APP;
use lazy_static::lazy_static;
use std::time::Duration;
use ureq::http::HeaderMap;

lazy_static! {
    pub static ref UREQ_AGENT: ureq::Agent = {
        let config = ureq::Agent::config_builder()
            .user_agent(concat!(
                env!("CARGO_PKG_NAME"),
                "/",
                env!("CARGO_PKG_VERSION")
            ))
            .timeout_global(Some(Duration::from_secs(120)))
            .build();
        config.into()
    };
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
