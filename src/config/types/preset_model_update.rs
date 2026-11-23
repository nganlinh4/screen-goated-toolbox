use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::ModelPriorityChains;

pub(crate) type PresetModelDefaults = BTreeMap<String, Vec<(String, String)>>;

/// Provider activation recommendations compiled from the shared model catalog.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(default)]
pub struct RecommendedProviderDefaults {
    pub use_groq: bool,
    pub use_gemini: bool,
    pub use_openrouter: bool,
    pub use_nvidia: bool,
    pub use_ollama: bool,
}

impl Default for RecommendedProviderDefaults {
    fn default() -> Self {
        Self {
            use_groq: crate::model_config::DEFAULT_USE_GROQ,
            use_gemini: crate::model_config::DEFAULT_USE_GEMINI,
            use_openrouter: crate::model_config::DEFAULT_USE_OPENROUTER,
            use_nvidia: crate::model_config::DEFAULT_USE_NVIDIA,
            use_ollama: crate::model_config::DEFAULT_USE_OLLAMA,
        }
    }
}

/// A staged update whose built-in recommendations must be compared after the
/// newly downloaded executable starts.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(default)]
pub struct PendingPresetModelUpdate {
    pub target_version: String,
    pub previous_models: BTreeMap<String, Vec<(String, String)>>,
    pub previous_model_priority_chains: Option<ModelPriorityChains>,
    pub previous_recommended_provider_defaults: Option<RecommendedProviderDefaults>,
}

#[cfg(all(test, not(feature = "recorder-worker")))]
mod tests {
    use super::{PendingPresetModelUpdate, RecommendedProviderDefaults};
    use crate::config::Config;

    #[test]
    fn nvidia_follows_the_catalog_recommendation_like_every_provider() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../parity-fixtures/preset-system/retry-runtime.json"
        ))
        .unwrap();
        let recommended = RecommendedProviderDefaults::default();
        assert_eq!(
            fixture["provider_settings"]["use_nvidia"].as_bool(),
            Some(recommended.use_nvidia)
        );

        let mut config = Config {
            use_nvidia: false,
            pending_preset_model_update: Some(PendingPresetModelUpdate {
                target_version: "2.0.0".to_string(),
                ..Default::default()
            }),
            ..Default::default()
        };
        config.finish_preset_model_update(true);
        assert_eq!(config.use_nvidia, recommended.use_nvidia);
    }

    #[test]
    fn a_changed_nvidia_recommendation_is_detected() {
        let mut previous = RecommendedProviderDefaults::default();
        previous.use_nvidia = !previous.use_nvidia;
        assert_ne!(previous, RecommendedProviderDefaults::default());
    }
}
