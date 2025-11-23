/// Centralized Model Configuration

#[derive(Clone, Debug, PartialEq)]
pub enum ModelType {
    Vision,
    Text,
}

#[derive(Clone, Debug)]
pub struct ModelConfig {
    pub id: String,
    pub provider: String,
    pub name_vi: String,
    pub name_en: String,
    pub full_name: String,
    pub model_type: ModelType,
    pub enabled: bool,
}

impl ModelConfig {
    pub fn new(
        id: &str,
        provider: &str,
        name_vi: &str,
        name_en: &str,
        full_name: &str,
        model_type: ModelType,
        enabled: bool,
    ) -> Self {
        Self {
            id: id.to_string(),
            provider: provider.to_string(),
            name_vi: name_vi.to_string(),
            name_en: name_en.to_string(),
            full_name: full_name.to_string(),
            model_type,
            enabled,
        }
    }

    pub fn get_label(&self, is_vietnamese: bool) -> String {
        let name = if is_vietnamese { &self.name_vi } else { &self.name_en };
        format!("{} ({})", name, self.full_name)
    }
}

pub fn get_all_models() -> Vec<ModelConfig> {
    vec![
        // --- VISION MODELS ---
        ModelConfig::new(
            "scout",
            "groq",
            "Nhanh",
            "Fast",
            "meta-llama/llama-4-scout-17b-16e-instruct",
            ModelType::Vision,
            true,
        ),
        ModelConfig::new(
            "maverick",
            "groq",
            "Chính xác",
            "Accurate",
            "meta-llama/llama-4-maverick-17b-128e-instruct",
            ModelType::Vision,
            true,
        ),
        ModelConfig::new(
            "gemini-flash-lite",
            "google",
            "Chính xác hơn",
            "More Accurate",
            "gemini-flash-lite-latest",
            ModelType::Vision,
            true,
        ),
        
        // --- TEXT MODELS (For Retranslate) ---
        ModelConfig::new(
            "fast_text",
            "groq",
            "Cực nhanh",
            "Super Fast",
            "openai/gpt-oss-20b",
            ModelType::Text,
            true,
        ),
    ]
}

pub fn get_model_by_id(id: &str) -> Option<ModelConfig> {
    get_all_models().into_iter().find(|m| m.id == id)
}
