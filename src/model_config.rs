/// Centralized model API backed by generated catalog data.

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ModelType {
    Vision,
    Text,
    Audio,
}

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
    pub quota_limit_vi: String,
    pub quota_limit_ko: String,
    pub quota_limit_en: String,
    pub source: ModelSource,
    pub supports_search_override: Option<bool>,
    pub quality_tier: Option<u8>,
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
        quota_limit_vi: &str,
        quota_limit_ko: &str,
        quota_limit_en: &str,
        quality_tier: u8,
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
            quota_limit_vi: quota_limit_vi.to_string(),
            quota_limit_ko: quota_limit_ko.to_string(),
            quota_limit_en: quota_limit_en.to_string(),
            source: ModelSource::BuiltIn,
            supports_search_override: None,
            quality_tier: Some(quality_tier),
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
    pub fn localized_quota(&self, lang: &str) -> &str {
        match lang {
            "vi" => &self.quota_limit_vi,
            "ko" => &self.quota_limit_ko,
            _ => &self.quota_limit_en,
        }
    }
}

include!(concat!(env!("OUT_DIR"), "/model_catalog_generated.rs"));

/// Check if a model is a non-LLM model (doesn't use prompts).
pub fn model_is_non_llm(model_id: &str) -> bool {
    GENERATED_NON_LLM_IDS.contains(&model_id)
}

static ALL_MODELS: LazyLock<Vec<ModelConfig>> = LazyLock::new(generated_models);

pub fn get_all_models() -> &'static [ModelConfig] {
    &ALL_MODELS[..]
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

    let cached = OLLAMA_MODEL_CACHE.lock().unwrap();
    cached.iter().find(|model| model.id == id).cloned()
}

pub fn live_endpoint_profile(api_model: &str) -> Option<LiveEndpointProfile> {
    generated_live_endpoint_profile(api_model)
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

pub fn normalize_realtime_transcription_model_id(model_id: &str) -> String {
    let normalized = generated_normalize_realtime_transcription_model_id(model_id);
    if normalized.starts_with("moonshine-") {
        DEFAULT_REALTIME_TRANSCRIPTION_MODEL.to_string()
    } else {
        normalized.to_string()
    }
}

pub fn realtime_transcription_api_model(model_id: &str) -> String {
    let normalized = normalize_realtime_transcription_model_id(model_id);
    get_model_by_id(&normalized)
        .map(|model| model.full_name)
        .unwrap_or_else(|| GEMINI_LIVE_API_MODEL_2_5.to_string())
}

pub fn realtime_transcription_live_protocol(model_id: &str) -> Option<&'static str> {
    let api_model = realtime_transcription_api_model(model_id);
    live_endpoint_profile(&api_model).and_then(|profile| profile.protocol)
}

pub fn is_gemini_live_translate_model_id(model_id: &str) -> bool {
    realtime_transcription_live_protocol(model_id) == Some("live-translate")
}

pub fn is_gemini_live_s2s_model_id(model_id: &str) -> bool {
    is_gemini_live_translate_model_id(model_id)
}

pub fn tts_gemini_model_options() -> &'static [(&'static str, &'static str)] {
    GENERATED_TTS_GEMINI_MODELS
}

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
pub fn get_all_models_with_ollama() -> Vec<ModelConfig> {
    let custom_models = crate::APP
        .lock()
        .ok()
        .map(|app| app.config.custom_models.clone())
        .unwrap_or_default();
    get_all_models_with_custom(&custom_models)
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

    let cached = OLLAMA_MODEL_CACHE.lock().unwrap();
    for ollama_model in cached.iter() {
        models.push(ollama_model.clone());
    }

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
        quota_limit_vi: custom.quota_vi.clone(),
        quota_limit_ko: custom.quota_ko.clone(),
        quota_limit_en: custom.quota_en.clone(),
        source: ModelSource::User,
        supports_search_override: custom.supports_search,
        quality_tier: None,
        typical_latency_ms: None,
        performance_source: None,
    })
}

/// Check if a model supports search capabilities by its Full Name (API Name).
pub fn model_supports_search_by_name(full_name: &str) -> bool {
    if GENERATED_SEARCH_DISABLED_FULL_NAMES
        .iter()
        .any(|blocked| full_name.contains(blocked))
    {
        return false;
    }

    if full_name.contains("gemini") {
        return true;
    }
    if full_name.contains("gemma") {
        return false;
    }
    if full_name.contains("compound") {
        return true;
    }

    false
}

/// Check if a model supports search capabilities by its Internal ID.
pub fn model_supports_search_by_id(id: &str) -> bool {
    let custom_models = crate::APP
        .lock()
        .ok()
        .map(|app| app.config.custom_models.clone())
        .unwrap_or_default();
    model_supports_search_by_id_with_custom(id, &custom_models)
}

pub fn model_supports_search_by_id_with_custom(
    id: &str,
    custom_models: &[crate::config::types::CustomModelDefinition],
) -> bool {
    if let Some(conf) = get_model_by_id_with_custom(id, custom_models) {
        if let Some(supports_search) = conf.supports_search_override {
            return supports_search;
        }
        return model_supports_search_by_name(&conf.full_name);
    }

    if id.contains("compound") {
        return true;
    }

    false
}

// === OLLAMA MODEL CACHE ===

use std::sync::{
    LazyLock, Mutex,
    atomic::{AtomicBool, Ordering},
};

/// Cached Ollama models (populated by background scan)
static OLLAMA_MODEL_CACHE: LazyLock<Mutex<Vec<ModelConfig>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

/// Whether a scan is currently in progress
static OLLAMA_SCAN_IN_PROGRESS: LazyLock<AtomicBool> = LazyLock::new(|| AtomicBool::new(false));

/// Last scan time (for debouncing) - initialized to 10s ago so first scan works immediately
static OLLAMA_LAST_SCAN: LazyLock<Mutex<std::time::Instant>> = LazyLock::new(|| {
    Mutex::new(
        std::time::Instant::now()
            .checked_sub(std::time::Duration::from_secs(10))
            .unwrap_or_else(std::time::Instant::now),
    )
});

/// Check if Ollama model scan is in progress
pub fn is_ollama_scan_in_progress() -> bool {
    OLLAMA_SCAN_IN_PROGRESS.load(Ordering::SeqCst)
}

pub fn ollama_cached_model_count() -> usize {
    OLLAMA_MODEL_CACHE
        .lock()
        .map(|models| models.len())
        .unwrap_or(0)
}

/// Trigger background scan for Ollama models (non-blocking)
/// Returns immediately, models will be populated in cache when ready
pub fn trigger_ollama_model_scan() {
    let (use_ollama, base_url) = if let Ok(app) = crate::APP.lock() {
        (app.config.use_ollama, app.config.ollama_base_url.clone())
    } else {
        return;
    };

    if !use_ollama {
        return;
    }

    {
        let last_scan = OLLAMA_LAST_SCAN.lock().unwrap();
        if last_scan.elapsed().as_secs() < 5 {
            return;
        }
    }

    if OLLAMA_SCAN_IN_PROGRESS.swap(true, Ordering::SeqCst) {
        return;
    }

    {
        let mut last_scan = OLLAMA_LAST_SCAN.lock().unwrap();
        *last_scan = std::time::Instant::now();
    }

    std::thread::spawn(move || {
        let result = crate::api::ollama::fetch_ollama_models_with_caps(&base_url);

        if let Ok(ollama_models) = result {
            let mut new_models = Vec::new();

            for ollama_model in ollama_models {
                let model_id = format!(
                    "ollama-{}",
                    ollama_model.name.replace(":", "-").replace("/", "-")
                );
                let display_name = format!("{} (Local)", ollama_model.name);

                if ollama_model.has_vision {
                    new_models.push(ModelConfig {
                        id: format!("{}-vision", model_id),
                        provider: "ollama".to_string(),
                        name_vi: display_name.clone(),
                        name_ko: display_name.clone(),
                        name_en: display_name.clone(),
                        full_name: ollama_model.name.clone(),
                        model_type: ModelType::Vision,
                        enabled: true,
                        quota_limit_vi: "Không giới hạn".to_string(),
                        quota_limit_ko: "무제한".to_string(),
                        quota_limit_en: "Unlimited".to_string(),
                        source: ModelSource::Discovered,
                        supports_search_override: None,
                        quality_tier: None,
                        typical_latency_ms: None,
                        performance_source: None,
                    });

                    new_models.push(ModelConfig {
                        id: model_id,
                        provider: "ollama".to_string(),
                        name_vi: display_name.clone(),
                        name_ko: display_name.clone(),
                        name_en: display_name.clone(),
                        full_name: ollama_model.name.clone(),
                        model_type: ModelType::Text,
                        enabled: true,
                        quota_limit_vi: "Không giới hạn".to_string(),
                        quota_limit_ko: "무제한".to_string(),
                        quota_limit_en: "Unlimited".to_string(),
                        source: ModelSource::Discovered,
                        supports_search_override: None,
                        quality_tier: None,
                        typical_latency_ms: None,
                        performance_source: None,
                    });
                } else {
                    new_models.push(ModelConfig {
                        id: model_id,
                        provider: "ollama".to_string(),
                        name_vi: display_name.clone(),
                        name_ko: display_name.clone(),
                        name_en: display_name,
                        full_name: ollama_model.name,
                        model_type: ModelType::Text,
                        enabled: true,
                        quota_limit_vi: "Không giới hạn".to_string(),
                        quota_limit_ko: "무제한".to_string(),
                        quota_limit_en: "Unlimited".to_string(),
                        source: ModelSource::Discovered,
                        supports_search_override: None,
                        quality_tier: None,
                        typical_latency_ms: None,
                        performance_source: None,
                    });
                }
            }

            let mut cache = OLLAMA_MODEL_CACHE.lock().unwrap();
            *cache = new_models;
        }

        OLLAMA_SCAN_IN_PROGRESS.store(false, Ordering::SeqCst);
    });
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;

    #[test]
    fn benchmark_winner_is_default_and_first_vision_fallback() {
        assert_eq!(
            DEFAULT_IMAGE_MODEL_ID,
            "google-gemini-3-5-flash-lite-vision"
        );
        assert_eq!(
            default_image_to_text_priority_chain_ids().first().copied(),
            Some(DEFAULT_IMAGE_MODEL_ID)
        );
        let model = get_model_by_id(DEFAULT_IMAGE_MODEL_ID).expect("default vision model exists");
        assert_eq!(model.provider, "google");
        assert_eq!(model.full_name, "gemini-3.5-flash-lite");
        assert_eq!(model.quality_tier, Some(5));
        assert_eq!(model.typical_latency_ms, Some(1500));
        assert_eq!(
            model.performance_source.as_deref(),
            Some("benchmark-2026-07-23:vision")
        );
    }

    #[test]
    fn live_thinking_schema_follows_exact_endpoint() {
        assert_eq!(
            live_endpoint_profile(GEMINI_LIVE_API_MODEL_2_5).and_then(|profile| profile.thinking),
            Some(LiveThinkingConfig::Budget(0))
        );
        assert_eq!(
            live_endpoint_profile(GEMINI_LIVE_API_MODEL_3_1).and_then(|profile| profile.thinking),
            Some(LiveThinkingConfig::Level("minimal"))
        );
    }

    #[test]
    fn live_output_limits_match_shared_parity_fixture() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/parity-fixtures/preset-system/gemini-live-socket-protocol.json"
        )))
        .expect("Gemini Live socket fixture parses");
        let limits = fixture["modelOutputLimits"]
            .as_object()
            .expect("modelOutputLimits must be an object");
        assert!(!limits.is_empty(), "modelOutputLimits must not be empty");

        for (api_model, expected) in limits {
            let expected = u32::try_from(
                expected
                    .as_u64()
                    .expect("model output limit must be an unsigned integer"),
            )
            .expect("model output limit must fit u32");
            assert_eq!(
                live_endpoint_profile(api_model).and_then(|profile| profile.max_output_tokens),
                Some(expected),
                "catalog output limit drifted for {api_model}"
            );
        }
    }

    #[test]
    fn tts_model_normalization_uses_catalog_default() {
        for persisted in ["", "gemini", "unknown-live-model"] {
            assert_eq!(
                normalize_tts_gemini_model(persisted),
                DEFAULT_GEMINI_LIVE_TTS_MODEL,
                "legacy or invalid TTS model must use the catalog default"
            );
        }

        for (api_model, _) in tts_gemini_model_options() {
            assert_eq!(normalize_tts_gemini_model(api_model), *api_model);
        }
    }

    #[test]
    fn live_translate_routing_comes_from_the_endpoint_profile() {
        assert_eq!(
            realtime_transcription_live_protocol(GEMINI_LIVE_TRANSLATE_MODEL_ID),
            Some("live-translate")
        );
        assert!(is_gemini_live_translate_model_id(
            GEMINI_LIVE_TRANSLATE_MODEL_ID
        ));
        assert!(!is_gemini_live_translate_model_id(
            GEMINI_LIVE_AUDIO_MODEL_ID_3_1
        ));
    }
}
