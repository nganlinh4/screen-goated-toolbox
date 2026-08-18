use crate::model_config::ModelConfig;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use ureq::http::HeaderMap;

pub const FRESH_THROUGH_SECONDS: u64 = 300;
pub const AGING_THROUGH_SECONDS: u64 = 900;
pub const LOCAL_RUNTIME_PROVIDERS: &[&str] = &["ollama", "parakeet", "qwen3", "moonshine"];

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

impl UsageKey {
    pub fn endpoint(provider: &str, full_name: &str) -> Self {
        Self {
            provider: normalize_provider(provider),
            scope: UsageScope::Endpoint(full_name.trim().to_string()),
        }
    }

    pub fn provider(provider: &str) -> Self {
        Self {
            provider: normalize_provider(provider),
            scope: UsageScope::Provider,
        }
    }
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

impl UsageMetricKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::RequestsDay => "RPD",
            Self::RequestsMinute => "RPM",
            Self::TokensMinute => "TPM",
            Self::TokensDay => "TPD",
            Self::AudioSecondsHour => "ASH",
            Self::AudioSecondsDay => "ASD",
        }
    }
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UsageFreshness {
    Fresh,
    Aging,
    Stale,
}

pub fn now_unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

pub fn freshness_at(observed_at: u64, now: u64) -> UsageFreshness {
    let age = now.saturating_sub(observed_at);
    if age <= FRESH_THROUGH_SECONDS {
        UsageFreshness::Fresh
    } else if age <= AGING_THROUGH_SECONDS {
        UsageFreshness::Aging
    } else {
        UsageFreshness::Stale
    }
}

pub fn usage_key_for_response(provider: &str, full_name: &str) -> UsageKey {
    if normalize_provider(provider) == "openrouter" {
        UsageKey::provider(provider)
    } else {
        UsageKey::endpoint(provider, full_name)
    }
}

pub fn snapshot_from_headers(
    provider: &str,
    headers: &HeaderMap,
    observed_at_unix_seconds: u64,
) -> Option<UsageSnapshot> {
    let provider = normalize_provider(provider);
    let mut metrics = Vec::new();

    if provider == "openrouter" {
        push_metric(
            &mut metrics,
            headers,
            UsageMetricKind::RequestsDay,
            HeaderTriple::new(
                "x-ratelimit-remaining",
                "x-ratelimit-limit",
                "x-ratelimit-reset",
            ),
        );
    } else {
        push_common_metrics(&mut metrics, headers);
    }

    push_metric(
        &mut metrics,
        headers,
        UsageMetricKind::AudioSecondsHour,
        HeaderTriple::new(
            "x-ratelimit-remaining-audio-seconds-hour",
            "x-ratelimit-limit-audio-seconds-hour",
            "x-ratelimit-reset-audio-seconds-hour",
        ),
    );
    push_metric(
        &mut metrics,
        headers,
        UsageMetricKind::AudioSecondsDay,
        HeaderTriple::new(
            "x-ratelimit-remaining-audio-seconds-day",
            "x-ratelimit-limit-audio-seconds-day",
            "x-ratelimit-reset-audio-seconds-day",
        ),
    );

    metrics.sort_by_key(|metric| metric.kind);
    (!metrics.is_empty()).then_some(UsageSnapshot {
        metrics,
        observed_at_unix_seconds,
    })
}

pub fn endpoint_representatives(models: &[ModelConfig]) -> Vec<&ModelConfig> {
    let mut representatives: HashMap<UsageKey, &ModelConfig> = HashMap::new();
    for model in models
        .iter()
        .filter(|model| model.enabled && provider_has_usage_statistics(&model.provider))
    {
        let key = UsageKey::endpoint(&model.provider, &model.full_name);
        representatives
            .entry(key)
            .and_modify(|current| {
                if model_order(model) < model_order(current) {
                    *current = model;
                }
            })
            .or_insert(model);
    }

    let mut result: Vec<_> = representatives.into_values().collect();
    result.sort_by(|left, right| model_order(left).cmp(&model_order(right)));
    result
}

pub fn provider_has_usage_statistics(provider: &str) -> bool {
    let normalized = provider.trim().to_ascii_lowercase();
    !LOCAL_RUNTIME_PROVIDERS.contains(&normalized.as_str())
}

fn normalize_provider(provider: &str) -> String {
    provider.trim().to_ascii_lowercase()
}

fn model_order(model: &ModelConfig) -> (u32, &str) {
    (model.typical_latency_ms.unwrap_or(u32::MAX), &model.id)
}

fn push_common_metrics(metrics: &mut Vec<UsageMetric>, headers: &HeaderMap) {
    push_metric(
        metrics,
        headers,
        UsageMetricKind::RequestsDay,
        HeaderTriple::new(
            "x-ratelimit-remaining-requests",
            "x-ratelimit-limit-requests",
            "x-ratelimit-reset-requests",
        ),
    );
    push_metric(
        metrics,
        headers,
        UsageMetricKind::TokensMinute,
        HeaderTriple::new(
            "x-ratelimit-remaining-tokens",
            "x-ratelimit-limit-tokens",
            "x-ratelimit-reset-tokens",
        ),
    );
}

#[derive(Clone, Copy)]
struct HeaderTriple {
    remaining: &'static str,
    limit: &'static str,
    reset: &'static str,
}

impl HeaderTriple {
    const fn new(remaining: &'static str, limit: &'static str, reset: &'static str) -> Self {
        Self {
            remaining,
            limit,
            reset,
        }
    }
}

fn push_metric(
    metrics: &mut Vec<UsageMetric>,
    headers: &HeaderMap,
    kind: UsageMetricKind,
    names: HeaderTriple,
) {
    let remaining = header_value(headers, names.remaining);
    let limit = header_value(headers, names.limit);
    if remaining.is_none() && limit.is_none() {
        return;
    }
    metrics.push(UsageMetric {
        kind,
        remaining,
        limit,
        reset: header_value(headers, names.reset),
    });
}

fn header_value(headers: &HeaderMap, name: &str) -> Option<String> {
    let value = headers.get(name)?.to_str().ok()?.trim();
    if value.is_empty() {
        return None;
    }
    Some(value.chars().take(80).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_config::{ModelConfig, ModelType};
    use serde_json::Value;

    fn model(id: &str, provider: &str, full_name: &str, latency: u32) -> ModelConfig {
        ModelConfig::new(
            id,
            provider,
            id,
            id,
            id,
            full_name,
            ModelType::Text,
            true,
            "10 lượt/ngày",
            "10회/일",
            "10 requests/day",
            false,
            false,
            1,
            latency,
            "test",
        )
    }

    #[test]
    fn roles_collapse_but_provider_identity_does_not() {
        let models = vec![
            model("demo-text", "demo", "vendor/model", 900),
            model("demo-vision", "demo", "vendor/model", 700),
            model("other-text", "other", "vendor/model", 600),
        ];
        let rows = endpoint_representatives(&models);
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().any(|model| model.id == "demo-vision"));
        assert!(rows.iter().any(|model| model.id == "other-text"));
    }

    #[test]
    fn local_runtime_models_never_become_api_usage_rows() {
        let models = vec![
            model("remote", "demo", "remote/model", 400),
            model("qwen", "qwen3", "Qwen3-ASR-0.6B", 300),
            model("parakeet", "parakeet", "parakeet-120m-v1", 200),
            model("ollama", "ollama", "local/model", 100),
            model("moonshine", "moonshine", "moonshine-small", 50),
        ];
        let rows = endpoint_representatives(&models);
        assert_eq!(
            rows.iter()
                .map(|model| model.id.as_str())
                .collect::<Vec<_>>(),
            ["remote"]
        );

        let fixture: Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/parity-fixtures/usage-statistics/contract.json"
        )))
        .unwrap();
        assert_eq!(
            fixture["local_runtime_providers"],
            serde_json::json!(LOCAL_RUNTIME_PROVIDERS)
        );
    }

    #[test]
    fn groq_headers_become_independent_request_and_token_metrics() {
        let mut headers = HeaderMap::new();
        headers.insert("x-ratelimit-remaining-requests", "19".parse().unwrap());
        headers.insert("x-ratelimit-limit-requests", "20".parse().unwrap());
        headers.insert("x-ratelimit-remaining-tokens", "5990".parse().unwrap());
        headers.insert("x-ratelimit-limit-tokens", "6000".parse().unwrap());

        let snapshot = snapshot_from_headers("groq", &headers, 123).unwrap();
        assert_eq!(snapshot.observed_at_unix_seconds, 123);
        assert_eq!(snapshot.metrics.len(), 2);
        assert_eq!(snapshot.metrics[0].kind, UsageMetricKind::RequestsDay);
        assert_eq!(snapshot.metrics[1].kind, UsageMetricKind::TokensMinute);
    }

    #[test]
    fn openrouter_headers_use_one_provider_scope() {
        assert_eq!(
            usage_key_for_response("openrouter", "first/model"),
            usage_key_for_response("openrouter", "second/model")
        );
        assert_ne!(
            usage_key_for_response("groq", "first/model"),
            usage_key_for_response("groq", "second/model")
        );
    }

    #[test]
    fn openrouter_provider_headers_parse_as_one_daily_request_bucket() {
        let mut headers = HeaderMap::new();
        headers.insert("x-ratelimit-remaining", "49".parse().unwrap());
        headers.insert("x-ratelimit-limit", "50".parse().unwrap());
        headers.insert("x-ratelimit-reset", "86400".parse().unwrap());

        let snapshot = snapshot_from_headers("openrouter", &headers, 123).unwrap();
        assert_eq!(snapshot.metrics.len(), 1);
        assert_eq!(snapshot.metrics[0].kind, UsageMetricKind::RequestsDay);
        assert_eq!(snapshot.metrics[0].remaining.as_deref(), Some("49"));
        assert_eq!(snapshot.metrics[0].limit.as_deref(), Some("50"));
    }

    #[test]
    fn freshness_thresholds_match_shared_contract() {
        assert_eq!(freshness_at(1_000, 1_300), UsageFreshness::Fresh);
        assert_eq!(freshness_at(1_000, 1_301), UsageFreshness::Aging);
        assert_eq!(freshness_at(1_000, 1_901), UsageFreshness::Stale);
    }

    #[test]
    fn shared_fixture_matches_runtime_thresholds_and_metric_order() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../parity-fixtures/usage-statistics/contract.json"
        ))
        .unwrap();
        assert_eq!(
            fixture["freshness"]["fresh_through_seconds"],
            FRESH_THROUGH_SECONDS
        );
        assert_eq!(
            fixture["freshness"]["aging_through_seconds"],
            AGING_THROUGH_SECONDS
        );
        assert_eq!(
            fixture["metric_order"],
            serde_json::json!([
                "requests_day",
                "requests_minute",
                "tokens_minute",
                "tokens_day",
                "audio_seconds_hour",
                "audio_seconds_day"
            ])
        );
        assert_eq!(
            fixture["observed_usage_providers"],
            serde_json::json!(["groq", "openrouter"])
        );
    }
}
