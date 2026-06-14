use std::collections::HashSet;

use crate::APP;
use crate::config::Config;
use crate::model_config::{
    ModelConfig, ModelType, get_all_models_with_custom, get_model_by_id_with_custom,
    model_is_non_llm,
};
use crate::retry_model_chain::{
    RetryChainKind, preflight_skip_reason, provider_is_available, resolve_next_retry_model,
};

use super::GTX_TRANSLATION_MODEL_ID;

pub(super) fn current_config() -> Result<Config, String> {
    APP.lock()
        .map(|app| app.config.clone())
        .map_err(|_| "App lock poisoned".to_string())
}

pub(super) fn localized_model_label(model: &ModelConfig, ui_language: &str) -> String {
    model.localized_name(ui_language).to_string()
}

pub(super) fn collect_translation_models(config: &Config) -> Vec<ModelConfig> {
    let blocked_providers = HashSet::new();
    let mut models = Vec::new();
    let mut seen_model_ids = HashSet::new();

    if let Some(initial_model) = resolve_initial_translation_model(config, &blocked_providers) {
        seen_model_ids.insert(initial_model.id.clone());
        models.push(initial_model.clone());
        let mut failed_model_ids = vec![initial_model.id.clone()];
        let mut current_model = initial_model;
        while let Some(next_model) = resolve_next_retry_model(
            &current_model.id,
            &failed_model_ids,
            &blocked_providers,
            RetryChainKind::TextToText,
            config,
        ) {
            if failed_model_ids
                .iter()
                .any(|failed| failed == &next_model.id)
            {
                break;
            }
            failed_model_ids.push(next_model.id.clone());
            current_model = next_model.clone();
            if seen_model_ids.insert(next_model.id.clone()) {
                models.push(next_model);
            }
        }
    }

    for model in get_all_models_with_custom(&config.custom_models)
        .into_iter()
        .filter(|model| is_compatible_translation_model(model, config, &blocked_providers))
    {
        if seen_model_ids.insert(model.id.clone()) {
            models.push(model);
        }
    }

    models
}

pub(super) fn collect_prioritized_translation_models(
    config: &Config,
    model_id: &str,
    smart_fallback: bool,
) -> Result<Vec<ModelConfig>, String> {
    if model_id == GTX_TRANSLATION_MODEL_ID {
        return Ok(if smart_fallback {
            collect_translation_models(config)
        } else {
            Vec::new()
        });
    }
    let blocked_providers = HashSet::new();
    let Some(model) = get_model_by_id_with_custom(model_id, &config.custom_models) else {
        return Err(format!("Unknown subtitle translation model: {model_id}"));
    };
    if !is_compatible_translation_model(&model, config, &blocked_providers) {
        return Err(format!(
            "Subtitle translation model '{}' is not currently available.",
            localized_model_label(&model, &config.ui_language)
        ));
    }
    let mut models = Vec::new();
    let mut seen_model_ids = HashSet::new();
    seen_model_ids.insert(model.id.clone());
    models.push(model);
    if smart_fallback {
        for fallback in collect_translation_models(config) {
            if seen_model_ids.insert(fallback.id.clone()) {
                models.push(fallback);
            }
        }
    }
    Ok(models)
}

fn resolve_initial_translation_model(
    config: &Config,
    blocked_providers: &HashSet<String>,
) -> Option<ModelConfig> {
    for candidate_id in RetryChainKind::TextToText.configured_chain(config) {
        let Some(model) = get_model_by_id_with_custom(candidate_id, &config.custom_models) else {
            continue;
        };
        if is_compatible_translation_model(&model, config, blocked_providers) {
            return Some(model);
        }
    }

    get_all_models_with_custom(&config.custom_models)
        .into_iter()
        .find(|model| is_compatible_translation_model(model, config, blocked_providers))
}

fn is_compatible_translation_model(
    model: &ModelConfig,
    config: &Config,
    blocked_providers: &HashSet<String>,
) -> bool {
    model.enabled
        && model.model_type == ModelType::Text
        && !model_is_non_llm(&model.id)
        && !blocked_providers.contains(&model.provider)
        && provider_is_available(&model.provider, config)
        && preflight_skip_reason(&model.id, &model.provider, config, blocked_providers).is_none()
}
