use crate::config::Config;
use crate::model_config::{
    ModelConfig, ModelType, get_all_models_with_custom, get_model_by_id_with_custom,
    model_is_non_llm, model_supports_search_by_id_with_custom,
    model_supports_search_by_provider_and_name,
};
use std::collections::{HashMap, HashSet};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

#[cfg(not(feature = "recorder-worker"))]
const MODEL_RATE_LIMIT_COOLDOWN: Duration = Duration::from_secs(300);
#[cfg(not(feature = "recorder-worker"))]
const MODEL_TIMEOUT_COOLDOWN: Duration = Duration::from_secs(30 * 60);
#[cfg(not(feature = "recorder-worker"))]
const MODEL_UNAVAILABLE_COOLDOWN: Duration = Duration::from_secs(6 * 60 * 60);
#[cfg(not(feature = "recorder-worker"))]
const MODEL_BILLING_COOLDOWN: Duration = Duration::from_secs(6 * 60 * 60);
#[cfg(not(feature = "recorder-worker"))]
const MODEL_TIMEOUT_FAILURE_THRESHOLD: u8 = 2;
#[cfg(not(feature = "recorder-worker"))]
const INTERACTIVE_TIMEOUT_MULTIPLIER: u64 = 10;
#[cfg(not(feature = "recorder-worker"))]
const MIN_INTERACTIVE_TIMEOUT_MS: u64 = 10_000;
#[cfg(not(feature = "recorder-worker"))]
const MAX_INTERACTIVE_TIMEOUT_MS: u64 = 30_000;

#[cfg(not(feature = "recorder-worker"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ModelCooldownKind {
    RateLimit,
    Timeout,
    Unavailable,
    Billing,
}

#[cfg(not(feature = "recorder-worker"))]
impl ModelCooldownKind {
    fn duration(self) -> Duration {
        match self {
            Self::RateLimit => MODEL_RATE_LIMIT_COOLDOWN,
            Self::Timeout => MODEL_TIMEOUT_COOLDOWN,
            Self::Unavailable => MODEL_UNAVAILABLE_COOLDOWN,
            Self::Billing => MODEL_BILLING_COOLDOWN,
        }
    }

    fn reason(self) -> &'static str {
        match self {
            Self::RateLimit => "MODEL_RATE_LIMIT_COOLDOWN",
            Self::Timeout => "MODEL_TIMEOUT_COOLDOWN",
            Self::Unavailable => "MODEL_UNAVAILABLE_COOLDOWN",
            Self::Billing => "MODEL_BILLING_COOLDOWN",
        }
    }
}

#[cfg(not(feature = "recorder-worker"))]
#[derive(Clone, Copy, Debug)]
enum ModelCircuitState {
    Monitoring {
        timeout_failures: u8,
    },
    Open {
        kind: ModelCooldownKind,
        until: Instant,
    },
    HalfOpen {
        kind: ModelCooldownKind,
    },
}

#[cfg(not(feature = "recorder-worker"))]
static MODEL_CIRCUITS: LazyLock<Mutex<HashMap<String, ModelCircuitState>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[cfg(feature = "recorder-worker")]
static MODEL_COOLDOWNS: LazyLock<Mutex<HashMap<String, Instant>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[cfg(not(feature = "recorder-worker"))]
fn rate_limit_error(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    lower.contains("http 429")
        || lower.contains("status code 429")
        || lower.contains("rate limit")
        || lower.contains("too many requests")
        || lower.contains("quota exceeded")
}

#[cfg(not(feature = "recorder-worker"))]
fn timeout_error(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    lower.contains("timeout") || lower.contains("timed out") || lower.contains("deadline exceeded")
}

#[cfg(not(feature = "recorder-worker"))]
fn billing_error(error: &str) -> bool {
    crate::overlay::utils::is_billing_exhausted_error(error)
}

#[cfg(not(feature = "recorder-worker"))]
fn unavailable_model_error(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    let is_404 = lower.contains("http 404")
        || lower.contains("status code 404")
        || lower.contains("error 404");
    if !is_404 || !(lower.contains("model") || lower.contains("deployment")) {
        return false;
    }
    lower.contains("unavailable")
        || lower.contains("archived")
        || lower.contains("not found")
        || lower.contains("no such model")
        || lower.contains("does not exist")
        || lower.contains("doesn't exist")
        || lower.contains("decommissioned")
        || lower.contains("deprecated")
        || lower.contains("has been removed")
        || lower.contains("was removed")
}

#[cfg(not(feature = "recorder-worker"))]
pub fn interactive_request_timeout(
    model_id: &str,
    config: &Config,
    streaming_enabled: bool,
) -> Option<Duration> {
    if streaming_enabled {
        return None;
    }

    let typical_latency_ms = get_model_by_id_with_custom(model_id, &config.custom_models)
        .and_then(|model| model.typical_latency_ms);
    Some(benchmark_derived_timeout(typical_latency_ms))
}

#[cfg(not(feature = "recorder-worker"))]
fn benchmark_derived_timeout(typical_latency_ms: Option<u32>) -> Duration {
    let timeout_ms = typical_latency_ms
        .map(u64::from)
        .map(|latency| latency.saturating_mul(INTERACTIVE_TIMEOUT_MULTIPLIER))
        .unwrap_or(MAX_INTERACTIVE_TIMEOUT_MS)
        .clamp(MIN_INTERACTIVE_TIMEOUT_MS, MAX_INTERACTIVE_TIMEOUT_MS);
    Duration::from_millis(timeout_ms)
}

#[cfg(not(feature = "recorder-worker"))]
pub fn record_model_failure(model_id: &str, error: &str) {
    record_model_failure_at(model_id, error, Instant::now());
}

#[cfg(not(feature = "recorder-worker"))]
fn record_model_failure_at(model_id: &str, error: &str, now: Instant) {
    let is_rate_limit = rate_limit_error(error);
    let is_timeout = timeout_error(error);
    let is_unavailable = unavailable_model_error(error);
    let is_billing = billing_error(error);
    if !is_rate_limit && !is_timeout && !is_unavailable && !is_billing {
        if let Ok(mut circuits) = MODEL_CIRCUITS.lock()
            && let Some(ModelCircuitState::HalfOpen { kind }) = circuits.get(model_id).copied()
        {
            circuits.insert(
                model_id.to_string(),
                ModelCircuitState::Open {
                    kind,
                    until: now + kind.duration(),
                },
            );
        }
        return;
    }

    let Ok(mut circuits) = MODEL_CIRCUITS.lock() else {
        return;
    };
    let state = circuits
        .entry(model_id.to_string())
        .or_insert(ModelCircuitState::Monitoring {
            timeout_failures: 0,
        });

    if matches!(*state, ModelCircuitState::Open { until, .. } if until <= now) {
        *state = ModelCircuitState::Monitoring {
            timeout_failures: 0,
        };
    }

    match *state {
        ModelCircuitState::Open { until, .. } if until > now => {}
        ModelCircuitState::HalfOpen { kind } => {
            let kind = if is_billing {
                ModelCooldownKind::Billing
            } else if is_rate_limit {
                ModelCooldownKind::RateLimit
            } else if is_timeout {
                ModelCooldownKind::Timeout
            } else if is_unavailable {
                ModelCooldownKind::Unavailable
            } else {
                kind
            };
            *state = ModelCircuitState::Open {
                kind,
                until: now + kind.duration(),
            };
        }
        ModelCircuitState::Monitoring { .. } if is_billing => {
            let kind = ModelCooldownKind::Billing;
            *state = ModelCircuitState::Open {
                kind,
                until: now + kind.duration(),
            };
        }
        ModelCircuitState::Monitoring { .. } if is_rate_limit => {
            let kind = ModelCooldownKind::RateLimit;
            *state = ModelCircuitState::Open {
                kind,
                until: now + kind.duration(),
            };
        }
        ModelCircuitState::Monitoring { .. } if is_unavailable => {
            let kind = ModelCooldownKind::Unavailable;
            *state = ModelCircuitState::Open {
                kind,
                until: now + kind.duration(),
            };
        }
        ModelCircuitState::Monitoring { timeout_failures } if is_timeout => {
            let timeout_failures = timeout_failures.saturating_add(1);
            if timeout_failures >= MODEL_TIMEOUT_FAILURE_THRESHOLD {
                let kind = ModelCooldownKind::Timeout;
                *state = ModelCircuitState::Open {
                    kind,
                    until: now + kind.duration(),
                };
            } else {
                *state = ModelCircuitState::Monitoring { timeout_failures };
            }
        }
        _ => {}
    }
}

#[cfg(not(feature = "recorder-worker"))]
pub fn record_model_success(model_id: &str) {
    if let Ok(mut circuits) = MODEL_CIRCUITS.lock() {
        circuits.remove(model_id);
    }
}

#[cfg(not(feature = "recorder-worker"))]
pub fn release_model_probe(model_id: &str) {
    if let Ok(mut circuits) = MODEL_CIRCUITS.lock()
        && let Some(ModelCircuitState::HalfOpen { kind }) = circuits.get(model_id).copied()
    {
        circuits.insert(
            model_id.to_string(),
            ModelCircuitState::Open {
                kind,
                until: Instant::now(),
            },
        );
    }
}

#[cfg(not(feature = "recorder-worker"))]
pub fn claim_model_attempt(model_id: &str) -> Option<String> {
    claim_model_attempt_at(model_id, Instant::now())
}

#[cfg(not(feature = "recorder-worker"))]
fn claim_model_attempt_at(model_id: &str, now: Instant) -> Option<String> {
    let mut circuits = MODEL_CIRCUITS.lock().ok()?;
    let state = circuits.get_mut(model_id)?;

    match *state {
        ModelCircuitState::Monitoring { .. } => None,
        ModelCircuitState::Open { kind, until } if until > now => Some(format!(
            "{}:{model_id}:{}s",
            kind.reason(),
            until.saturating_duration_since(now).as_secs().max(1)
        )),
        ModelCircuitState::Open { kind, .. } => {
            *state = ModelCircuitState::HalfOpen { kind };
            None
        }
        ModelCircuitState::HalfOpen { .. } => {
            Some(format!("MODEL_COOLDOWN_PROBE_IN_FLIGHT:{model_id}"))
        }
    }
}

#[cfg(not(feature = "recorder-worker"))]
fn model_cooldown_skip_reason(model_id: &str) -> Option<String> {
    model_cooldown_skip_reason_at(model_id, Instant::now())
}

#[cfg(feature = "recorder-worker")]
fn model_cooldown_remaining(model_id: &str) -> Option<Duration> {
    let now = Instant::now();
    let mut cooldowns = MODEL_COOLDOWNS.lock().ok()?;
    cooldowns.retain(|_, until| *until > now);
    cooldowns
        .get(model_id)
        .map(|until| until.saturating_duration_since(now))
}

#[cfg(not(feature = "recorder-worker"))]
fn model_cooldown_skip_reason_at(model_id: &str, now: Instant) -> Option<String> {
    let mut circuits = MODEL_CIRCUITS.lock().ok()?;
    let state = circuits.get_mut(model_id)?;

    match *state {
        ModelCircuitState::Monitoring { .. } => None,
        ModelCircuitState::Open { kind, until } if until > now => Some(format!(
            "{}:{model_id}:{}s",
            kind.reason(),
            until.saturating_duration_since(now).as_secs().max(1)
        )),
        ModelCircuitState::Open { .. } => None,
        ModelCircuitState::HalfOpen { .. } => {
            Some(format!("MODEL_COOLDOWN_PROBE_IN_FLIGHT:{model_id}"))
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetryChainKind {
    #[cfg(not(feature = "recorder-worker"))]
    ImageToText,
    TextToText,
}

impl RetryChainKind {
    #[cfg(not(feature = "recorder-worker"))]
    pub fn from_block_type(block_type: &str) -> Option<Self> {
        match block_type {
            "image" => Some(Self::ImageToText),
            "text" => Some(Self::TextToText),
            _ => None,
        }
    }

    pub fn target_model_type(self) -> ModelType {
        match self {
            #[cfg(not(feature = "recorder-worker"))]
            Self::ImageToText => ModelType::Vision,
            Self::TextToText => ModelType::Text,
        }
    }

    pub fn configured_chain(self, config: &Config) -> &[String] {
        match self {
            #[cfg(not(feature = "recorder-worker"))]
            Self::ImageToText => &config.model_priority_chains.image_to_text,
            Self::TextToText => &config.model_priority_chains.text_to_text,
        }
    }
}

pub fn provider_is_available(provider: &str, config: &Config) -> bool {
    match provider {
        "groq" => config.use_groq && credential_present("GROQ_API_KEY", &config.api_key),
        "google" | "gemini-live" => {
            config.use_gemini && credential_present("GEMINI_API_KEY", &config.gemini_api_key)
        }
        "openrouter" => {
            config.use_openrouter
                && credential_present("OPENROUTER_API_KEY", &config.openrouter_api_key)
        }
        "ollama" => config.use_ollama,
        "google-gtx" | "qrserver" | "parakeet" | "taalas" => true,
        _ => false,
    }
}

fn provider_preflight_skip_reason(provider: &str, config: &Config) -> Option<String> {
    match provider {
        "groq" => {
            if !config.use_groq {
                Some("PROVIDER_DISABLED:groq".to_string())
            } else if !credential_present("GROQ_API_KEY", &config.api_key) {
                Some("NO_API_KEY:groq".to_string())
            } else {
                None
            }
        }
        "google" | "gemini-live" => {
            if !config.use_gemini {
                Some(format!("PROVIDER_DISABLED:{provider}"))
            } else if !credential_present("GEMINI_API_KEY", &config.gemini_api_key) {
                Some(format!("NO_API_KEY:{provider}"))
            } else {
                None
            }
        }
        "openrouter" => {
            if !config.use_openrouter {
                Some("PROVIDER_DISABLED:openrouter".to_string())
            } else if !credential_present("OPENROUTER_API_KEY", &config.openrouter_api_key) {
                Some("NO_API_KEY:openrouter".to_string())
            } else {
                None
            }
        }
        "ollama" => (!config.use_ollama).then_some("PROVIDER_DISABLED:ollama".to_string()),
        "google-gtx" | "qrserver" | "parakeet" | "taalas" => None,
        _ => Some(format!("Provider {provider} is disabled.")),
    }
}

fn credential_present(environment: &str, saved: &str) -> bool {
    !crate::api::provider_credentials::resolve(environment, saved).is_empty()
}

pub fn preflight_skip_reason(
    model_id: &str,
    provider: &str,
    config: &Config,
    blocked_providers: &HashSet<String>,
) -> Option<String> {
    #[cfg(not(feature = "recorder-worker"))]
    if let Some(reason) = model_cooldown_skip_reason(model_id) {
        return Some(reason);
    }
    #[cfg(feature = "recorder-worker")]
    if let Some(remaining) = model_cooldown_remaining(model_id) {
        return Some(format!(
            "MODEL_RATE_LIMIT_COOLDOWN:{model_id}:{}s",
            remaining.as_secs().max(1)
        ));
    }
    if blocked_providers.contains(provider) {
        return Some(format!("Provider {} is unavailable for retry.", provider));
    }

    if let Some(reason) = provider_preflight_skip_reason(provider, config) {
        return Some(reason);
    }

    if get_model_by_id_with_custom(model_id, &config.custom_models).is_none() {
        return Some(format!("Model config not found: {}", model_id));
    }

    None
}

pub fn resolve_next_retry_model(
    current_model_id: &str,
    failed_model_ids: &[String],
    blocked_providers: &HashSet<String>,
    chain_kind: RetryChainKind,
    config: &Config,
) -> Option<ModelConfig> {
    resolve_next_configured_model(
        current_model_id,
        failed_model_ids,
        blocked_providers,
        chain_kind,
        config,
    )
    .or_else(|| {
        let must_support_search =
            model_supports_search_by_id_with_custom(current_model_id, &config.custom_models);
        resolve_auto_retry_model(
            current_model_id,
            failed_model_ids,
            blocked_providers,
            &chain_kind.target_model_type(),
            must_support_search,
            config,
        )
    })
}

pub fn resolve_next_configured_model(
    current_model_id: &str,
    failed_model_ids: &[String],
    blocked_providers: &HashSet<String>,
    chain_kind: RetryChainKind,
    config: &Config,
) -> Option<ModelConfig> {
    let must_support_search =
        model_supports_search_by_id_with_custom(current_model_id, &config.custom_models);

    for candidate_id in chain_kind.configured_chain(config) {
        if failed_model_ids
            .iter()
            .any(|failed_id| failed_id == candidate_id)
        {
            continue;
        }

        let Some(model) = get_model_by_id_with_custom(candidate_id, &config.custom_models) else {
            continue;
        };

        if is_retry_candidate_compatible(
            &model,
            &chain_kind.target_model_type(),
            must_support_search,
            blocked_providers,
            config,
        ) {
            return Some(model);
        }
    }
    None
}

fn resolve_auto_retry_model(
    current_model_id: &str,
    failed_model_ids: &[String],
    blocked_providers: &HashSet<String>,
    current_model_type: &ModelType,
    must_support_search: bool,
    config: &Config,
) -> Option<ModelConfig> {
    let all_models: Vec<ModelConfig> = get_all_models_with_custom(&config.custom_models);
    let current_provider = get_model_by_id_with_custom(current_model_id, &config.custom_models)
        .map(|m| m.provider)
        .unwrap_or_default();

    let same_provider_candidates: Vec<&ModelConfig> = all_models
        .iter()
        .filter(|model| {
            model.provider == current_provider
                && model.id != current_model_id
                && !failed_model_ids
                    .iter()
                    .any(|failed_id| failed_id == &model.id)
                && is_retry_candidate_compatible(
                    model,
                    current_model_type,
                    must_support_search,
                    blocked_providers,
                    config,
                )
        })
        .collect();

    if let Some(last) = same_provider_candidates.last() {
        return Some((*last).clone());
    }

    let diff_provider_candidates: Vec<&ModelConfig> = all_models
        .iter()
        .filter(|model| {
            model.provider != current_provider
                && !failed_model_ids
                    .iter()
                    .any(|failed_id| failed_id == &model.id)
                && is_retry_candidate_compatible(
                    model,
                    current_model_type,
                    must_support_search,
                    blocked_providers,
                    config,
                )
        })
        .collect();

    diff_provider_candidates
        .last()
        .map(|model| (*model).clone())
}

fn is_retry_candidate_compatible(
    model: &ModelConfig,
    current_model_type: &ModelType,
    must_support_search: bool,
    blocked_providers: &HashSet<String>,
    config: &Config,
) -> bool {
    model.enabled
        && model.model_type == *current_model_type
        && !model_is_non_llm(&model.id)
        && !blocked_providers.contains(&model.provider)
        && provider_is_available(&model.provider, config)
        && (!must_support_search
            || model.supports_search_override.unwrap_or_else(|| {
                model_supports_search_by_provider_and_name(&model.provider, &model.full_name)
            }))
}

#[cfg(all(test, not(feature = "recorder-worker")))]
mod tests;
