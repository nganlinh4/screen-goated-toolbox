/// Centralized Model Configuration

#[derive(Clone, Debug, PartialEq)]
pub enum ModelType {
    Vision,
    Text,
    Audio,
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
}

impl ModelConfig {
    pub fn new(
        id: &str,
        provider: &str,
        name_vi: &str,
        name_ko: &str,
        name_en: &str,
        full_name: &str,
        model_type: ModelType,
        enabled: bool,
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
        }
    }

    pub fn get_label(&self, ui_language: &str) -> String {
        let name = match ui_language {
            "vi" => &self.name_vi,
            "ko" => &self.name_ko,
            _ => &self.name_en,
        };
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
            "빠름",
            "Fast",
            "meta-llama/llama-4-scout-17b-16e-instruct",
            ModelType::Vision,
            true,
        ),
        ModelConfig::new(
            "maverick",
            "groq",
            "Chính xác",
            "정확함",
            "Accurate",
            "meta-llama/llama-4-maverick-17b-128e-instruct",
            ModelType::Vision,
            true,
        ),
        ModelConfig::new(
            "gemini-flash-lite",
            "google",
            "Chính xác hơn",
            "더 정확함",
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
            "초고속",
            "Super Fast",
            "openai/gpt-oss-20b",
            ModelType::Text,
            true,
        ),

        // --- AUDIO MODELS ---
        ModelConfig::new(
            "whisper-fast",
            "groq",
            "Nhanh",
            "빠름",
            "Fast",
            "whisper-large-v3-turbo",
            ModelType::Audio,
            true,
        ),
        ModelConfig::new(
            "whisper-accurate",
            "groq",
            "Chính xác",
            "정확함",
            "Accurate",
            "whisper-large-v3",
            ModelType::Audio,
            true,
        ),
    ]
}

pub fn get_model_by_id(id: &str) -> Option<ModelConfig> {
    get_all_models().into_iter().find(|m| m.id == id)
}
