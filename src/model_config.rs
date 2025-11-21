/// Centralized Model Configuration
/// 
/// This module manages models from multiple providers.
/// Models can be easily added/removed here.

#[derive(Clone, Debug)]
pub struct ModelConfig {
    pub id: String,
    pub provider: String,
    pub name_vi: String,
    pub name_en: String,
    pub full_name: String,
    pub enabled: bool,
}

impl ModelConfig {
    pub fn new(
        id: &str,
        provider: &str,
        name_vi: &str,
        name_en: &str,
        full_name: &str,
        enabled: bool,
    ) -> Self {
        Self {
            id: id.to_string(),
            provider: provider.to_string(),
            name_vi: name_vi.to_string(),
            name_en: name_en.to_string(),
            full_name: full_name.to_string(),
            enabled,
        }
    }

    pub fn get_label(&self, is_vietnamese: bool) -> String {
        if self.enabled {
            if is_vietnamese {
                format!("{} ({})", self.name_vi, self.full_name)
            } else {
                format!("{} ({})", self.name_en, self.full_name)
            }
        } else {
            if is_vietnamese {
                format!("{} ({})", self.name_vi, self.full_name)
            } else {
                format!("{} ({})", self.name_en, self.full_name)
            }
        }
    }

    pub fn get_label_short(&self, is_vietnamese: bool) -> String {
        if is_vietnamese {
            self.name_vi.clone()
        } else {
            self.name_en.clone()
        }
    }
}

/// Get all available models
pub fn get_all_models() -> Vec<ModelConfig> {
    vec![
        ModelConfig::new(
            "scout",
            "groq",
            "Nhanh",
            "Fast",
            "meta-llama/llama-4-scout-17b-16e-instruct",
            true,
        ),
        ModelConfig::new(
            "maverick",
            "groq",
            "Chính xác",
            "Accurate",
            "meta-llama/llama-4-maverick-17b-128e-instruct",
            true,
        ),
        ModelConfig::new(
            "gemini-flash-lite",
            "google",
            "Chính xác hơn",
            "More Accurate",
            "gemini-flash-lite-latest",
            false, // Upcoming, disabled
        ),
    ]
}

/// Find a model by ID
pub fn get_model_by_id(id: &str) -> Option<ModelConfig> {
    get_all_models().into_iter().find(|m| m.id == id)
}

pub struct ModelSelector {
    preferred_model: String,
}

impl ModelSelector {
    /// Create a new model selector with preferred model
    pub fn new(preferred_model: String) -> Self {
        Self { preferred_model }
    }

    /// Get the model to use
    ///
    /// Returns the model name as a string
    pub fn get_model(&self) -> String {
        get_model_by_id(&self.preferred_model)
            .map(|m| m.full_name)
            .unwrap_or_else(|| "meta-llama/llama-4-scout-17b-16e-instruct".to_string())
    }

    /// Update the preferred model
    pub fn set_preferred_model(&mut self, model: String) {
        self.preferred_model = model;
    }
}