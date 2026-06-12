use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct CustomModelDefinition {
    pub id: String,
    pub provider: String,
    pub display_name: String,
    pub full_name: String,
    pub model_type: CustomModelType,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default = "default_quota_en")]
    pub quota_en: String,
    #[serde(default = "default_quota_vi")]
    pub quota_vi: String,
    #[serde(default = "default_quota_ko")]
    pub quota_ko: String,
    #[serde(default)]
    pub supports_search: Option<bool>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum CustomModelType {
    Text,
    Vision,
}

impl Default for CustomModelDefinition {
    fn default() -> Self {
        Self {
            id: String::new(),
            provider: "openrouter".to_string(),
            display_name: String::new(),
            full_name: String::new(),
            model_type: CustomModelType::Text,
            enabled: true,
            quota_en: default_quota_en(),
            quota_vi: default_quota_vi(),
            quota_ko: default_quota_ko(),
            supports_search: None,
        }
    }
}

fn default_enabled() -> bool {
    true
}

fn default_quota_en() -> String {
    "Provider quota".to_string()
}

fn default_quota_vi() -> String {
    "Theo nhà cung cấp".to_string()
}

fn default_quota_ko() -> String {
    "공급자 기준".to_string()
}
