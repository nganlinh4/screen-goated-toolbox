use crate::config::Config;
use crate::model_config::{
    ModelConfig, ModelType, get_all_models_with_ollama, get_model_by_id, model_is_non_llm,
    model_supports_search_by_id, model_supports_search_by_name,
};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetryChainKind {
    ImageToText,
    TextToText,
}

impl RetryChainKind {
    pub fn from_block_type(block_type: &str) -> Option<Self> {
        match block_type {
            "image" => Some(Self::ImageToText),
            "text" => Some(Self::TextToText),
            _ => None,
        }
    }

    pub fn target_model_type(self) -> ModelType {
        match self {
            Self::ImageToText => ModelType::Vision,
            Self::TextToText => ModelType::Text,
        }
    }

    pub fn configured_chain(self, config: &Config) -> &[String] {
        match self {
            Self::ImageToText => &config.model_priority_chains.image_to_text,
            Self::TextToText => &config.model_priority_chains.text_to_text,
        }
    }
}

pub fn provider_is_available(provider: &str, config: &Config) -> bool {
    match provider {
        "groq" => config.use_groq && !config.api_key.trim().is_empty(),
        "google" | "gemini-live" => config.use_gemini && !config.gemini_api_key.trim().is_empty(),
        "openrouter" => config.use_openrouter && !config.openrouter_api_key.trim().is_empty(),
        "cerebras" => config.use_cerebras && !config.cerebras_api_key.trim().is_empty(),
        "ollama" => config.use_ollama,
        "google-gtx" | "qrserver" | "parakeet" => true,
        _ => false,
    }
}

fn provider_preflight_skip_reason(provider: &str, config: &Config) -> Option<String> {
    match provider {
        "groq" => {
            if !config.use_groq {
                Some("PROVIDER_DISABLED:groq".to_string())
            } else if config.api_key.trim().is_empty() {
                Some("NO_API_KEY:groq".to_string())
            } else {
                None
            }
        }
        "google" | "gemini-live" => {
            if !config.use_gemini {
                Some(format!("PROVIDER_DISABLED:{provider}"))
            } else if config.gemini_api_key.trim().is_empty() {
                Some(format!("NO_API_KEY:{provider}"))
            } else {
                None
            }
        }
        "openrouter" => {
            if !config.use_openrouter {
                Some("PROVIDER_DISABLED:openrouter".to_string())
            } else if config.openrouter_api_key.trim().is_empty() {
                Some("NO_API_KEY:openrouter".to_string())
            } else {
                None
            }
        }
        "cerebras" => {
            if !config.use_cerebras {
                Some("PROVIDER_DISABLED:cerebras".to_string())
            } else if config.cerebras_api_key.trim().is_empty() {
                Some("NO_API_KEY:cerebras".to_string())
            } else {
                None
            }
        }
        "ollama" => (!config.use_ollama).then_some("PROVIDER_DISABLED:ollama".to_string()),
        "google-gtx" | "qrserver" | "parakeet" => None,
        _ => Some(format!("Provider {provider} is disabled.")),
    }
}

pub fn preflight_skip_reason(
    model_id: &str,
    provider: &str,
    config: &Config,
    blocked_providers: &HashSet<String>,
) -> Option<String> {
    if blocked_providers.contains(provider) {
        return Some(format!("Provider {} is unavailable for retry.", provider));
    }

    if let Some(reason) = provider_preflight_skip_reason(provider, config) {
        return Some(reason);
    }

    if get_model_by_id(model_id).is_none() {
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
    let must_support_search = model_supports_search_by_id(current_model_id);

    for candidate_id in chain_kind.configured_chain(config) {
        if failed_model_ids
            .iter()
            .any(|failed_id| failed_id == candidate_id)
        {
            continue;
        }

        let Some(model) = get_model_by_id(candidate_id) else {
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

    resolve_auto_retry_model(
        current_model_id,
        failed_model_ids,
        blocked_providers,
        &chain_kind.target_model_type(),
        must_support_search,
        config,
    )
}

fn resolve_auto_retry_model(
    current_model_id: &str,
    failed_model_ids: &[String],
    blocked_providers: &HashSet<String>,
    current_model_type: &ModelType,
    must_support_search: bool,
    config: &Config,
) -> Option<ModelConfig> {
    let all_models = get_all_models_with_ollama();
    let current_provider = get_model_by_id(current_model_id)
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
        && (!must_support_search || model_supports_search_by_name(&model.full_name))
}

#[cfg(test)]
mod tests {
    use super::{RetryChainKind, preflight_skip_reason, resolve_next_retry_model};
    use crate::config::Config;
    use std::collections::HashSet;

    #[test]
    fn skips_disabled_provider_in_preflight() {
        let config = Config {
            use_gemini: false,
            ..Default::default()
        };

        let reason = preflight_skip_reason(
            "gemini-3.1-flash-lite-preview",
            "google",
            &config,
            &HashSet::new(),
        );

        assert_eq!(reason.as_deref(), Some("PROVIDER_DISABLED:google"));
    }

    #[test]
    fn explicit_chain_respects_failed_models() {
        let config = Config::default();
        let failed = vec!["scout".to_string()];

        let next = resolve_next_retry_model(
            "scout",
            &failed,
            &HashSet::new(),
            RetryChainKind::ImageToText,
            &config,
        )
        .expect("image chain should produce a next model");

        assert_eq!(next.id, "gemini-3.1-flash-lite-preview");
    }
}
