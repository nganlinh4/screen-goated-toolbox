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
    pub quota_limit: String, 
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
        quota_limit: &str,
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
            quota_limit: quota_limit.to_string(),
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

    pub fn get_name_only(&self, ui_language: &str) -> String {
        match ui_language {
            "vi" => self.name_vi.clone(),
            "ko" => self.name_ko.clone(),
            _ => self.name_en.clone(),
        }
    }
}

pub fn get_all_models() -> Vec<ModelConfig> {
    vec![
        ModelConfig::new(
            "scout",
            "groq",
            "Nhanh",
            "빠름",
            "Fast",
            "meta-llama/llama-4-scout-17b-16e-instruct",
            ModelType::Vision,
            true,
            "1000 lượt/ngày"
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
            "1000 lượt/ngày"
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
            "1000 lượt/ngày"
        ),
        ModelConfig::new(
            "gemini-flash",
            "google",
            "Rất chính xác",
            "매우 정확함",
            "Very Accurate",
            "gemini-flash-latest",
            ModelType::Vision,
            true,
            "250 lượt/ngày"
        ),
        ModelConfig::new(
            "gemini-pro",
            "google",
            "Siêu chính xác, chậm",
            "초정밀, 느림",
            "Super Accurate, Slow",
            "gemini-2.5-pro",
            ModelType::Vision,
            true,
            "50 lượt/ngày"
        ),
        ModelConfig::new(
            "fast_text",
            "groq",
            "Cực nhanh",
            "초고속",
            "Super Fast",
            "openai/gpt-oss-20b",
            ModelType::Text,
            true,
            "1000 lượt/ngày"
        ),
        ModelConfig::new(
            "whisper-fast",
            "groq",
            "Nhanh",
            "빠름",
            "Fast",
            "whisper-large-v3-turbo",
            ModelType::Audio,
            true,
            "8h audio/ngày"
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
            "8h audio/ngày"
        ),
        ModelConfig::new(
            "gemini-audio",
            "google",
            "Chính xác hơn",
            "더 정확함",
            "More Accurate",
            "gemini-flash-lite-latest",
            ModelType::Audio,
            true,
            "1000 lượt/ngày"
        ),
        ModelConfig::new(
            "gemini-audio-flash",
            "google",
            "Rất chính xác",
            "매우 정확함",
            "Very Accurate",
            "gemini-flash-latest",
            ModelType::Audio,
            true,
            "250 lượt/ngày"
        ),
        ModelConfig::new(
            "gemini-audio-pro",
            "google",
            "Siêu chính xác, chậm",
            "초정밀, 느림",
            "Super Accurate, Slow",
            "gemini-2.5-pro",
            ModelType::Audio,
            true,
            "50 lượt/ngày"
        ),
    ]
}

pub fn get_model_by_id(id: &str) -> Option<ModelConfig> {
    get_all_models().into_iter().find(|m| m.id == id)
}
