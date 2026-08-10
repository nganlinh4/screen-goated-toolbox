use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use ureq::http::HeaderMap;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum UsageScope {
    Provider,
    Endpoint(String),
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct UsageKey {
    pub provider: String,
    pub scope: UsageScope,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum UsageMetricKind {
    RequestsDay,
    RequestsMinute,
    TokensMinute,
    TokensDay,
    AudioSecondsHour,
    AudioSecondsDay,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UsageMetric {
    pub kind: UsageMetricKind,
    pub remaining: Option<String>,
    pub limit: Option<String>,
    pub reset: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UsageSnapshot {
    pub metrics: Vec<UsageMetric>,
    pub observed_at_unix_seconds: u64,
}

pub type UsageStore = HashMap<UsageKey, UsageSnapshot>;

pub fn now_unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

pub fn usage_key_for_response(provider: &str, full_name: &str) -> UsageKey {
    if normalize_provider(provider) == "openrouter" {
        UsageKey {
            provider: normalize_provider(provider),
            scope: UsageScope::Provider,
        }
    } else {
        UsageKey {
            provider: normalize_provider(provider),
            scope: UsageScope::Endpoint(full_name.trim().to_string()),
        }
    }
}

pub fn snapshot_from_headers(
    provider: &str,
    headers: &HeaderMap,
    observed_at_unix_seconds: u64,
) -> Option<UsageSnapshot> {
    let provider = normalize_provider(provider);
    let mut metrics = Vec::new();
    if provider == "cerebras" {
        push(
            &mut metrics,
            headers,
            UsageMetricKind::RequestsDay,
            "x-ratelimit-remaining-requests-day",
            "x-ratelimit-limit-requests-day",
            "x-ratelimit-reset-requests-day",
        );
        push(
            &mut metrics,
            headers,
            UsageMetricKind::RequestsMinute,
            "x-ratelimit-remaining-requests-minute",
            "x-ratelimit-limit-requests-minute",
            "x-ratelimit-reset-requests-minute",
        );
        push(
            &mut metrics,
            headers,
            UsageMetricKind::TokensMinute,
            "x-ratelimit-remaining-tokens-minute",
            "x-ratelimit-limit-tokens-minute",
            "x-ratelimit-reset-tokens-minute",
        );
        push(
            &mut metrics,
            headers,
            UsageMetricKind::TokensDay,
            "x-ratelimit-remaining-tokens-day",
            "x-ratelimit-limit-tokens-day",
            "x-ratelimit-reset-tokens-day",
        );
        if metrics.is_empty() {
            push_common(&mut metrics, headers);
        }
    } else if provider == "openrouter" {
        push(
            &mut metrics,
            headers,
            UsageMetricKind::RequestsDay,
            "x-ratelimit-remaining",
            "x-ratelimit-limit",
            "x-ratelimit-reset",
        );
    } else {
        push_common(&mut metrics, headers);
    }
    push(
        &mut metrics,
        headers,
        UsageMetricKind::AudioSecondsHour,
        "x-ratelimit-remaining-audio-seconds-hour",
        "x-ratelimit-limit-audio-seconds-hour",
        "x-ratelimit-reset-audio-seconds-hour",
    );
    push(
        &mut metrics,
        headers,
        UsageMetricKind::AudioSecondsDay,
        "x-ratelimit-remaining-audio-seconds-day",
        "x-ratelimit-limit-audio-seconds-day",
        "x-ratelimit-reset-audio-seconds-day",
    );
    metrics.sort_by_key(|metric| metric.kind);
    (!metrics.is_empty()).then_some(UsageSnapshot {
        metrics,
        observed_at_unix_seconds,
    })
}

fn push_common(metrics: &mut Vec<UsageMetric>, headers: &HeaderMap) {
    push(
        metrics,
        headers,
        UsageMetricKind::RequestsDay,
        "x-ratelimit-remaining-requests",
        "x-ratelimit-limit-requests",
        "x-ratelimit-reset-requests",
    );
    push(
        metrics,
        headers,
        UsageMetricKind::TokensMinute,
        "x-ratelimit-remaining-tokens",
        "x-ratelimit-limit-tokens",
        "x-ratelimit-reset-tokens",
    );
}

fn push(
    metrics: &mut Vec<UsageMetric>,
    headers: &HeaderMap,
    kind: UsageMetricKind,
    remaining: &str,
    limit: &str,
    reset: &str,
) {
    let remaining = header(headers, remaining);
    let limit = header(headers, limit);
    if remaining.is_some() || limit.is_some() {
        metrics.push(UsageMetric {
            kind,
            remaining,
            limit,
            reset: header(headers, reset),
        });
    }
}

fn header(headers: &HeaderMap, name: &str) -> Option<String> {
    let value = headers.get(name)?.to_str().ok()?.trim();
    (!value.is_empty()).then(|| value.chars().take(80).collect())
}

fn normalize_provider(provider: &str) -> String {
    provider.trim().to_ascii_lowercase()
}
