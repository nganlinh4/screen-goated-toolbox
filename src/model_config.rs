/// Centralized model API backed by generated catalog data.
use std::sync::LazyLock;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ModelType {
    Vision,
    Text,
    Audio,
}

#[cfg(not(feature = "recorder-worker"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelSource {
    BuiltIn,
    User,
    Discovered,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiveThinkingConfig {
    Budget(u64),
    Level(&'static str),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrdinaryReasoningPolicy {
    NotApplicable,
    GeminiBudget(u32),
    GeminiLevel(&'static str),
    OpenAiEffort(&'static str),
    ProviderManaged,
    LiveProfile,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum VisionInputOrder {
    TextFirst,
    ImageFirst,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum VisionMediaResolutionPolicy {
    ProviderDefault,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum VisionSamplingPolicy {
    ProviderDefault,
    Qwen3GroqNonThinking,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum StructuredOutputPolicy {
    Unsupported,
    // Constructed by the generated catalog rather than by hand, so it reads as
    // dead whenever no enabled endpoint selects it. It stays because the value
    // is part of the shared wire contract that Android, both validators, and
    // catalog/README.md all define.
    #[allow(dead_code)]
    PromptOnly,
    JsonObject,
    StrictJsonSchema,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct VisionRequestProfile {
    pub input_order: VisionInputOrder,
    pub media_resolution: VisionMediaResolutionPolicy,
    pub sampling: VisionSamplingPolicy,
    pub max_output_tokens: Option<u32>,
    pub structured_output: StructuredOutputPolicy,
}

impl VisionRequestProfile {
    const SAFE_DEFAULT: Self = Self {
        input_order: VisionInputOrder::TextFirst,
        media_resolution: VisionMediaResolutionPolicy::ProviderDefault,
        sampling: VisionSamplingPolicy::ProviderDefault,
        max_output_tokens: None,
        structured_output: StructuredOutputPolicy::Unsupported,
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LiveEndpointProfile {
    pub lifecycle: &'static str,
    pub thinking: Option<LiveThinkingConfig>,
    pub max_output_tokens: Option<u32>,
    pub automatic_activity_detection_default: bool,
    pub protocol: Option<&'static str>,
}

#[derive(Clone, Debug)]
pub struct ModelConfig {
    pub id: String,
    pub provider: String,
    pub name_vi: String,
    pub name_ko: String,
    pub name_en: String,
    pub full_name: String,
    pub model_type: ModelType,
    pub enabled: bool,
    #[cfg(not(feature = "recorder-worker"))]
    pub quota_limit_vi: String,
    #[cfg(not(feature = "recorder-worker"))]
    pub quota_limit_ko: String,
    #[cfg(not(feature = "recorder-worker"))]
    pub quota_limit_en: String,
    #[cfg(not(feature = "recorder-worker"))]
    pub source: ModelSource,
    pub supports_search_override: Option<bool>,
    #[cfg(not(feature = "recorder-worker"))]
    pub search_tool_enabled_by_default: bool,
    pub intelligence_tier: Option<u8>,
    pub typical_latency_ms: Option<u32>,
    pub performance_source: Option<String>,
}

impl ModelConfig {
    #[expect(
        clippy::too_many_arguments,
        reason = "constructor mirrors the static model catalog fields directly"
    )]
    pub fn new(
        id: &str,
        provider: &str,
        name_vi: &str,
        name_ko: &str,
        name_en: &str,
        full_name: &str,
        model_type: ModelType,
        enabled: bool,
        _quota_limit_vi: &str,
        _quota_limit_ko: &str,
        _quota_limit_en: &str,
        supports_search: bool,
        _search_tool_enabled_by_default: bool,
        intelligence_tier: u8,
        typical_latency_ms: u32,
        performance_source: &str,
    ) -> Self {
        Self {
            id: id.to_string(),
            provider: provider.to_string(),
            name_vi: name_vi.to_string(),
            name_ko: name_ko.to_string(),
            name_en: name_en.to_string(),
            full_name: full_name.to_string(),
            model_type,
            enabled,
            #[cfg(not(feature = "recorder-worker"))]
            quota_limit_vi: _quota_limit_vi.to_string(),
            #[cfg(not(feature = "recorder-worker"))]
            quota_limit_ko: _quota_limit_ko.to_string(),
            #[cfg(not(feature = "recorder-worker"))]
            quota_limit_en: _quota_limit_en.to_string(),
            #[cfg(not(feature = "recorder-worker"))]
            source: ModelSource::BuiltIn,
            supports_search_override: Some(supports_search),
            #[cfg(not(feature = "recorder-worker"))]
            search_tool_enabled_by_default: _search_tool_enabled_by_default,
            intelligence_tier: Some(intelligence_tier),
            typical_latency_ms: Some(typical_latency_ms),
            performance_source: Some(performance_source.to_string()),
        }
    }

    /// Display name for the given UI language (`vi`/`ko`, else English).
    pub fn localized_name(&self, lang: &str) -> &str {
        match lang {
            "vi" => &self.name_vi,
            "ko" => &self.name_ko,
            _ => &self.name_en,
        }
    }

    /// Quota label for the given UI language (`vi`/`ko`, else English).
    #[cfg(not(feature = "recorder-worker"))]
    pub fn localized_quota(&self, lang: &str) -> &str {
        match lang {
            "vi" => &self.quota_limit_vi,
            "ko" => &self.quota_limit_ko,
            _ => &self.quota_limit_en,
        }
    }
}

include!(concat!(env!("OUT_DIR"), "/model_catalog_generated.rs"));

#[path = "model_config/presentation.rs"]
mod presentation;
pub use presentation::sort_models_for_display;
#[path = "model_config/ollama.rs"]
mod ollama;
pub use ollama::trigger_ollama_model_scan;
#[cfg(not(feature = "recorder-worker"))]
pub use ollama::{is_ollama_scan_in_progress, ollama_cached_model_count};
#[cfg(all(test, not(feature = "recorder-worker")))]
#[path = "model_config/realtime_routing_tests.rs"]
mod realtime_routing_tests;
#[cfg(all(test, not(feature = "recorder-worker")))]
#[path = "model_config/tests.rs"]
mod tests;

/// Check if a model is a non-LLM model (doesn't use prompts).
pub fn model_is_non_llm(model_id: &str) -> bool {
    GENERATED_NON_LLM_IDS.contains(&model_id)
}

static ALL_MODELS: LazyLock<Vec<ModelConfig>> = LazyLock::new(generated_models);

pub fn get_all_models() -> &'static [ModelConfig] {
    &ALL_MODELS[..]
}

/// Canonical provider name for prose surfaces: the usage dashboard, API-key
/// notifications, and provider-attributed error text. These exact strings are
/// locked by the API-key notification parity fixture.
///
/// `openai` and `anthropic` are not catalog providers, but they appear in
/// upstream error payloads and need the same spelling everywhere.
#[cfg(not(feature = "recorder-worker"))]
pub fn provider_full_name(provider: &str) -> &str {
    match provider {
        "google" | "gemini-live" => "Gemini",
        "google-gtx" => "Google Translate",
        "groq" => "Groq",
        "openrouter" => "OpenRouter",
        "nvidia" => "NVIDIA",
        "ollama" => "Ollama",
        "qrserver" => "QR",
        "parakeet" => "Parakeet",
        "qwen3" => "Qwen Local",
        "taalas" => "Taalas",
        "openai" => "OpenAI",
        "anthropic" => "Anthropic",
        other => other,
    }
}

/// Short provider name for compact surfaces such as the result badge and the
/// custom-models dialog. Identical to [`provider_full_name`] except for the local
/// runtimes, which collapse to a single label rather than naming each engine.
#[cfg(not(feature = "recorder-worker"))]
pub fn provider_display_name(provider: &str) -> &str {
    match provider {
        "parakeet" | "qwen3" => "Local",
        other => provider_full_name(other),
    }
}

pub fn get_model_by_id(id: &str) -> Option<ModelConfig> {
    let custom_models = crate::APP
        .lock()
        .ok()
        .map(|app| app.config.custom_models.clone())
        .unwrap_or_default();
    get_model_by_id_with_custom(id, &custom_models)
}

pub fn get_model_by_id_with_custom(
    id: &str,
    custom_models: &[crate::config::types::CustomModelDefinition],
) -> Option<ModelConfig> {
    if let Some(model) = get_all_models().iter().find(|m| m.id == id) {
        return Some(model.clone());
    }

    if let Some(model) = custom_models
        .iter()
        .filter_map(custom_model_definition_to_config)
        .find(|model| model.id == id)
    {
        return Some(model);
    }

    ollama::find_cached_model(id)
}

pub fn live_endpoint_profile(api_model: &str) -> Option<LiveEndpointProfile> {
    generated_live_endpoint_profile(api_model)
}

pub fn ordinary_reasoning_policy(provider: &str, api_model: &str) -> OrdinaryReasoningPolicy {
    generated_ordinary_reasoning_policy(provider, api_model)
}

pub fn vision_request_profile(provider: &str, api_model: &str) -> VisionRequestProfile {
    generated_vision_request_profile(provider, api_model)
        .unwrap_or(VisionRequestProfile::SAFE_DEFAULT)
}

fn live_thinking_json(config: Option<LiveThinkingConfig>) -> Option<serde_json::Value> {
    match config {
        Some(LiveThinkingConfig::Budget(value)) => {
            Some(serde_json::json!({ "thinkingBudget": value }))
        }
        Some(LiveThinkingConfig::Level(value)) => {
            Some(serde_json::json!({ "thinkingLevel": value }))
        }
        None => None,
    }
}

pub fn apply_live_generation_config(generation_config: &mut serde_json::Value, api_model: &str) {
    let Some(profile) = live_endpoint_profile(api_model) else {
        return;
    };
    if let Some(limit) = profile.max_output_tokens {
        generation_config["maxOutputTokens"] = limit.into();
    }
    if let Some(config) = live_thinking_json(profile.thinking) {
        generation_config["thinkingConfig"] = config;
    }
}

#[cfg(not(feature = "recorder-worker"))]
pub fn normalize_realtime_transcription_model_id(model_id: &str) -> String {
    let normalized = generated_normalize_realtime_transcription_model_id(model_id);
    if normalized.starts_with("moonshine-") {
        DEFAULT_REALTIME_TRANSCRIPTION_MODEL.to_string()
    } else {
        normalized.to_string()
    }
}

#[cfg(not(feature = "recorder-worker"))]
pub fn realtime_transcription_api_model(model_id: &str) -> String {
    let normalized = normalize_realtime_transcription_model_id(model_id);
    get_all_models()
        .iter()
        .find(|model| model.id == normalized)
        .map(|model| model.full_name.clone())
        .unwrap_or_else(|| GEMINI_LIVE_API_MODEL_2_5.to_string())
}

#[cfg(not(feature = "recorder-worker"))]
pub fn realtime_transcription_live_protocol(model_id: &str) -> Option<&'static str> {
    let api_model = realtime_transcription_api_model(model_id);
    live_endpoint_profile(&api_model).and_then(|profile| profile.protocol)
}

#[cfg(not(feature = "recorder-worker"))]
pub fn is_gemini_live_translate_model_id(model_id: &str) -> bool {
    realtime_transcription_live_protocol(model_id) == Some("live-translate")
}

#[cfg(not(feature = "recorder-worker"))]
pub fn is_gemini_live_s2s_model_id(model_id: &str) -> bool {
    is_gemini_live_translate_model_id(model_id)
}

pub fn tts_gemini_model_options() -> &'static [(&'static str, &'static str)] {
    GENERATED_TTS_GEMINI_MODELS
}

#[cfg(not(feature = "recorder-worker"))]
pub fn realtime_transcription_model_options() -> &'static [(&'static str, &'static str)] {
    GENERATED_REALTIME_TRANSCRIPTION_OPTIONS
}

pub fn normalize_tts_gemini_model(api_model: &str) -> &'static str {
    GENERATED_TTS_GEMINI_MODELS
        .iter()
        .find(|(candidate, _)| *candidate == api_model)
        .map(|(candidate, _)| *candidate)
        .unwrap_or(DEFAULT_GEMINI_LIVE_TTS_MODEL)
}

pub fn default_image_to_text_priority_chain_ids() -> &'static [&'static str] {
    DEFAULT_IMAGE_TO_TEXT_PRIORITY_CHAIN_IDS
}

pub fn default_text_to_text_priority_chain_ids() -> &'static [&'static str] {
    DEFAULT_TEXT_TO_TEXT_PRIORITY_CHAIN_IDS
}

/// Get all models including dynamically fetched Ollama models.
#[cfg(not(feature = "recorder-worker"))]
pub fn get_all_models_with_ollama() -> Vec<ModelConfig> {
    let custom_models = crate::APP
        .lock()
        .ok()
        .map(|app| app.config.custom_models.clone())
        .unwrap_or_default();
    let mut models = get_all_models_with_custom(&custom_models);
    sort_models_for_display(&mut models);
    models
}

pub fn get_all_models_with_custom(
    custom_models: &[crate::config::types::CustomModelDefinition],
) -> Vec<ModelConfig> {
    let mut models: Vec<ModelConfig> = ALL_MODELS.iter().cloned().collect();

    models.extend(
        custom_models
            .iter()
            .filter_map(custom_model_definition_to_config),
    );

    models.extend(ollama::cached_models());

    models
}

pub fn custom_model_definition_to_config(
    custom: &crate::config::types::CustomModelDefinition,
) -> Option<ModelConfig> {
    let id = custom.id.trim();
    let provider = custom.provider.trim();
    let full_name = custom.full_name.trim();
    if id.is_empty() || provider.is_empty() || full_name.is_empty() {
        return None;
    }

    let model_type = match custom.model_type {
        crate::config::types::CustomModelType::Text => ModelType::Text,
        crate::config::types::CustomModelType::Vision => ModelType::Vision,
    };
    let display_name = if custom.display_name.trim().is_empty() {
        full_name
    } else {
        custom.display_name.trim()
    };

    Some(ModelConfig {
        id: id.to_string(),
        provider: provider.to_string(),
        name_vi: display_name.to_string(),
        name_ko: display_name.to_string(),
        name_en: display_name.to_string(),
        full_name: full_name.to_string(),
        model_type,
        enabled: custom.enabled,
        #[cfg(not(feature = "recorder-worker"))]
        quota_limit_vi: custom.quota_vi.clone(),
        #[cfg(not(feature = "recorder-worker"))]
        quota_limit_ko: custom.quota_ko.clone(),
        #[cfg(not(feature = "recorder-worker"))]
        quota_limit_en: custom.quota_en.clone(),
        #[cfg(not(feature = "recorder-worker"))]
        source: ModelSource::User,
        supports_search_override: custom.supports_search,
        #[cfg(not(feature = "recorder-worker"))]
        search_tool_enabled_by_default: false,
        intelligence_tier: None,
        typical_latency_ms: None,
        performance_source: None,
    })
}

/// Check if a provider endpoint supports search capabilities.
pub fn model_supports_search_by_provider_and_name(provider: &str, full_name: &str) -> bool {
    get_all_models()
        .iter()
        .find(|model| model.provider == provider && model.full_name == full_name)
        .and_then(|model| model.supports_search_override)
        .unwrap_or(false)
}

pub fn model_supports_search_by_id_with_custom(
    id: &str,
    custom_models: &[crate::config::types::CustomModelDefinition],
) -> bool {
    if let Some(conf) = get_model_by_id_with_custom(id, custom_models) {
        if let Some(supports_search) = conf.supports_search_override {
            return supports_search;
        }
        return model_supports_search_by_provider_and_name(&conf.provider, &conf.full_name);
    }

    false
}

/// Whether normal selection of this model actually enables its search tool.
///
/// This is intentionally separate from provider capability: an endpoint may
/// support quota-bearing search while ordinary requests keep the tool off.
#[cfg(not(feature = "recorder-worker"))]
pub fn model_search_tool_enabled_by_default_by_id(id: &str) -> bool {
    let custom_models = crate::APP
        .lock()
        .ok()
        .map(|app| app.config.custom_models.clone())
        .unwrap_or_default();
    model_search_tool_enabled_by_default_by_id_with_custom(id, &custom_models)
}

#[cfg(not(feature = "recorder-worker"))]
pub fn model_search_tool_enabled_by_default_by_id_with_custom(
    id: &str,
    custom_models: &[crate::config::types::CustomModelDefinition],
) -> bool {
    get_model_by_id_with_custom(id, custom_models)
        .is_some_and(|model| model.search_tool_enabled_by_default)
}
