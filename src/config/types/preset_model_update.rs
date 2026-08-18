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
    pub use_ollama: bool,
}

impl Default for RecommendedProviderDefaults {
    fn default() -> Self {
        Self {
            use_groq: crate::model_config::DEFAULT_USE_GROQ,
            use_gemini: crate::model_config::DEFAULT_USE_GEMINI,
            use_openrouter: crate::model_config::DEFAULT_USE_OPENROUTER,
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
