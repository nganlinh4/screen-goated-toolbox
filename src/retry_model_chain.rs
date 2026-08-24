use crate::config::Config;
use crate::model_config::{
    ModelConfig, ModelType, get_all_models_with_custom, get_model_by_id_with_custom,
    model_is_non_llm, model_supports_search_by_id_with_custom,
    model_supports_search_by_provider_and_name,
};
use std::collections::HashSet;
#[cfg(not(feature = "recorder-worker"))]
use std::time::Duration;

#[cfg(not(feature = "recorder-worker"))]
#[path = "retry_model_chain/budget.rs"]
mod budget;
#[path = "retry_model_chain/cooldown.rs"]
mod cooldown;

#[cfg(not(feature = "recorder-worker"))]
const INTERACTIVE_STARTUP_ALLOWANCE_MS: u64 = 30_000;
#[cfg(not(feature = "recorder-worker"))]
const INTERACTIVE_REQUEST_BYTES_PER_SECOND: u64 = 16_384;
#[cfg(not(feature = "recorder-worker"))]
const MAX_INTERACTIVE_REQUEST_ALLOWANCE_MS: u64 = 120_000;
#[cfg(not(feature = "recorder-worker"))]
const MIN_INTERACTIVE_OUTPUT_TOKENS_PER_SECOND: u64 = 16;
#[cfg(not(feature = "recorder-worker"))]
const DEFAULT_TEXT_OUTPUT_TOKENS: u64 = 4_096;
#[cfg(not(feature = "recorder-worker"))]
const DEFAULT_VISION_OUTPUT_TOKENS: u64 = 2_048;
#[cfg(not(feature = "recorder-worker"))]
const MIN_INTERACTIVE_TIMEOUT_MS: u64 = 60_000;
#[cfg(not(feature = "recorder-worker"))]
const MAX_INTERACTIVE_TIMEOUT_MS: u64 = 900_000;
const UNBENCHMARKED_FEED_QUALITY_TIER: u8 = 4;

#[cfg(feature = "recorder-worker")]
use cooldown::model_cooldown_remaining;
#[cfg(not(feature = "recorder-worker"))]
use cooldown::model_cooldown_skip_reason;
#[cfg(not(feature = "recorder-worker"))]
pub use cooldown::{
    claim_model_attempt, record_model_failure, record_model_success, release_model_probe,
};

/// Stores the token balance a provider reported for one endpoint.
///
/// Only the per-minute token window is tracked; request-count windows are far
/// larger than a single call and are already handled by the cooldown.
#[cfg(not(feature = "recorder-worker"))]
pub fn record_token_budget(provider: &str, api_model: &str, headers: &ureq::http::HeaderMap) {
    // Counts are plain integers; only the reset carries a duration suffix.
    let count =
        |name: &str| -> Option<u32> { headers.get(name)?.to_str().ok()?.trim().parse().ok() };
    let reset = headers
        .get("x-ratelimit-reset-tokens")
        .and_then(|value| value.to_str().ok())
        .and_then(cooldown::parse_duration_seconds);
    let (Some(limit), Some(remaining), Some(reset)) = (
        count("x-ratelimit-limit-tokens"),
        count("x-ratelimit-remaining-tokens"),
        reset,
    ) else {
        return;
    };
    budget::record(
        &budget_key(provider, api_model),
        limit,
        remaining,
        Duration::from_secs_f64(reset),
    );
}

/// Key a token budget by the exact provider-qualified endpoint.
#[cfg(not(feature = "recorder-worker"))]
fn budget_key(provider: &str, api_model: &str) -> String {
    format!("{}:{}", provider.trim(), api_model.trim())
}

/// Cheapest total this endpoint could be billed for one vision call, or `None`
/// when the catalog has no measured floor to compare against.
#[cfg(not(feature = "recorder-worker"))]
fn minimum_vision_request_tokens(provider: &str, api_model: &str) -> Option<u32> {
    let profile = crate::model_config::vision_request_profile(provider, api_model);
    profile
        .max_output_tokens
        .map(|reserve| budget::MEASURED_MIN_IMAGE_TOKENS + reserve)
}

#[cfg(not(feature = "recorder-worker"))]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct InteractiveRequestWorkload {
    pub encoded_request_bytes: u64,
}

#[cfg(not(feature = "recorder-worker"))]
pub fn interactive_request_timeout(
    model_id: &str,
    config: &Config,
    streaming_enabled: bool,
    workload: InteractiveRequestWorkload,
) -> Option<Duration> {
    if streaming_enabled {
        return None;
    }

    let model = get_model_by_id_with_custom(model_id, &config.custom_models);
    let output_tokens = model
        .as_ref()
        .map(|model| match model.model_type {
            ModelType::Vision => {
                crate::model_config::vision_request_profile(&model.provider, &model.full_name)
                    .max_output_tokens
                    .map(u64::from)
                    .unwrap_or(DEFAULT_VISION_OUTPUT_TOKENS)
            }
            ModelType::Text | ModelType::Audio => DEFAULT_TEXT_OUTPUT_TOKENS,
        })
        .unwrap_or(DEFAULT_TEXT_OUTPUT_TOKENS);
    Some(workload_derived_timeout(
        workload.encoded_request_bytes,
        output_tokens,
    ))
}

#[cfg(not(feature = "recorder-worker"))]
fn workload_derived_timeout(encoded_request_bytes: u64, output_tokens: u64) -> Duration {
    let request_seconds = encoded_request_bytes
        .saturating_add(INTERACTIVE_REQUEST_BYTES_PER_SECOND - 1)
        / INTERACTIVE_REQUEST_BYTES_PER_SECOND;
    let request_allowance_ms = request_seconds
        .saturating_mul(1_000)
        .min(MAX_INTERACTIVE_REQUEST_ALLOWANCE_MS);
    let output_ms = output_tokens
        .saturating_mul(1_000)
        .saturating_add(MIN_INTERACTIVE_OUTPUT_TOKENS_PER_SECOND - 1)
        / MIN_INTERACTIVE_OUTPUT_TOKENS_PER_SECOND;
    let timeout_ms = INTERACTIVE_STARTUP_ALLOWANCE_MS
        .saturating_add(request_allowance_ms)
        .saturating_add(output_ms)
        .clamp(MIN_INTERACTIVE_TIMEOUT_MS, MAX_INTERACTIVE_TIMEOUT_MS);
    Duration::from_millis(timeout_ms)
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

    pub fn adaptive_enabled(self, config: &Config) -> bool {
        match self {
            #[cfg(not(feature = "recorder-worker"))]
            Self::ImageToText => config.adaptive_model_priority.image_to_text,
            Self::TextToText => config.adaptive_model_priority.text_to_text,
        }
    }

    pub fn live_overrides(self, config: &Config) -> &crate::config::types::LiveModelOverrides {
        match self {
            #[cfg(not(feature = "recorder-worker"))]
            Self::ImageToText => &config.adaptive_model_priority.image_to_text_overrides,
            Self::TextToText => &config.adaptive_model_priority.text_to_text_overrides,
        }
    }

    /// The chain to actually walk, with eligible availability-feed models
    /// interleaved below the local head by quality-adjusted latency.
    ///
    /// The feed can lengthen the fallback but never take first contact: position
    /// 0 is tied to the configured default and stays local. With no feed, no
    /// credential, or the provider disabled, this is the configured chain
    /// unchanged.
    pub fn effective_chain(self, config: &Config) -> Vec<String> {
        let configured = self.configured_chain(config);
        if !self.adaptive_enabled(config) {
            return configured.to_vec();
        }
        self.adaptive_chain(config, configured, self.live_overrides(config))
    }

    /// Resolves an adaptive chain from editor-owned rows without cloning the
    /// rest of [`Config`]. The priority UI holds those rows mutably, while feed
    /// eligibility and model ranks still come from the same frame's config.
    pub(crate) fn adaptive_chain(
        self,
        config: &Config,
        configured: &[String],
        overrides: &crate::config::types::LiveModelOverrides,
    ) -> Vec<String> {
        let offered = crate::model_feed::store::offered_models(config, self.target_model_type());
        let offered_ids: Vec<String> = offered.iter().map(|(id, _)| id.clone()).collect();
        if offered_ids.is_empty() {
            configured.to_vec()
        } else {
            crate::model_feed::merge_into_chain_with_overrides(
                configured,
                &offered_ids,
                &overrides.pinned,
                &overrides.excluded,
                |id| {
                    let model =
                        crate::model_config::get_model_by_id_with_custom(id, &config.custom_models);
                    crate::model_feed::CandidateRank {
                        // A live-only model has operational evidence but no durable
                        // cross-provider catalog benchmark yet. Tier 4 keeps it
                        // useful without inventing premium quality evidence.
                        quality_tier: model
                            .as_ref()
                            .and_then(|model| model.intelligence_tier)
                            .unwrap_or(UNBENCHMARKED_FEED_QUALITY_TIER),
                        latency_ms: offered
                            .iter()
                            .find_map(|(offered_id, latency)| {
                                (offered_id == id).then_some(*latency)
                            })
                            .or_else(|| model.and_then(|model| model.typical_latency_ms))
                            .unwrap_or(u32::MAX),
                    }
                },
            )
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
        "nvidia" => {
            config.use_nvidia && credential_present("NVIDIA_API_KEY", &config.nvidia_api_key)
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
        "nvidia" => {
            if !config.use_nvidia {
                Some("PROVIDER_DISABLED:nvidia".to_string())
            } else if !credential_present("NVIDIA_API_KEY", &config.nvidia_api_key) {
                Some("NO_API_KEY:nvidia".to_string())
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

/// Why this model should be passed over for the request about to be made.
///
/// `input_pixels` is the area of the image this call would send, where there is
/// one. An endpoint that declares a reliable floor is skipped below it, and the
/// caller advances to the next model exactly as it would for another structural
/// capability mismatch.
pub fn preflight_skip_reason(
    model_id: &str,
    provider: &str,
    config: &Config,
    blocked_providers: &HashSet<String>,
    input_pixels: Option<u32>,
) -> Option<String> {
    #[cfg(feature = "recorder-worker")]
    let _ = input_pixels;
    #[cfg(not(feature = "recorder-worker"))]
    if let Some(pixels) = input_pixels
        && let Some(model) = get_model_by_id_with_custom(model_id, &config.custom_models)
        && let Some(floor) =
            crate::model_config::vision_request_profile(&model.provider, &model.full_name)
                .min_reliable_pixels
        && pixels < floor
    {
        return Some(format!("MODEL_INPUT_TOO_SMALL:{model_id}:{pixels}px"));
    }
    #[cfg(not(feature = "recorder-worker"))]
    if let Some(reason) = model_cooldown_skip_reason(model_id) {
        return Some(reason);
    }
    // A window that cannot cover even the cheapest call this endpoint accepts
    // will reject the next one, so skip it rather than pay for the rejection.
    #[cfg(not(feature = "recorder-worker"))]
    if let Some(model) = get_model_by_id_with_custom(model_id, &config.custom_models)
        && let Some(minimum) = minimum_vision_request_tokens(&model.provider, &model.full_name)
        && let Some(wait) =
            budget::shortfall(&budget_key(&model.provider, &model.full_name), minimum)
    {
        return Some(format!(
            "MODEL_TOKEN_BUDGET:{model_id}:{}s",
            wait.as_secs().max(1)
        ));
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

/// Selects the first usable fallback when a preset's pinned model no longer
/// resolves. The preset remains unchanged; execution silently enters the same
/// priority and compatibility machinery used after an ordinary model failure.
#[cfg(not(feature = "recorder-worker"))]
pub fn resolve_unavailable_pinned_model(
    block_type: &str,
    unavailable_model_id: &str,
    config: &Config,
) -> Option<ModelConfig> {
    let chain_kind = RetryChainKind::from_block_type(block_type)?;
    resolve_next_retry_model(
        unavailable_model_id,
        &[unavailable_model_id.to_string()],
        &HashSet::new(),
        chain_kind,
        config,
    )
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

    let chain = chain_kind.effective_chain(config);
    for candidate_id in &chain {
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
        && (must_support_search || !model_requires_search_tool(model))
        && (!must_support_search
            || model.supports_search_override.unwrap_or_else(|| {
                model_supports_search_by_provider_and_name(&model.provider, &model.full_name)
            }))
}

fn model_requires_search_tool(model: &ModelConfig) -> bool {
    #[cfg(not(feature = "recorder-worker"))]
    {
        model.search_tool_enabled_by_default
    }
    #[cfg(feature = "recorder-worker")]
    {
        let _ = model;
        false
    }
}

#[cfg(all(test, not(feature = "recorder-worker")))]
mod tests;
