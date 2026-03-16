use serde::{Deserialize, Serialize};

fn default_image_to_text_priority_chain() -> Vec<String> {
    vec![
        "gemini-3.1-flash-lite-preview".to_string(),
        "scout".to_string(),
    ]
}

fn default_text_to_text_priority_chain() -> Vec<String> {
    vec![
        crate::model_config::DEFAULT_CEREBRAS_TEXT_MODEL_ID.to_string(),
        "text_accurate_kimi".to_string(),
        "text_gemini_3_1_flash_lite".to_string(),
    ]
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ModelPriorityChains {
    #[serde(default = "default_image_to_text_priority_chain")]
    pub image_to_text: Vec<String>,
    #[serde(default = "default_text_to_text_priority_chain")]
    pub text_to_text: Vec<String>,
}

impl Default for ModelPriorityChains {
    fn default() -> Self {
        Self {
            image_to_text: default_image_to_text_priority_chain(),
            text_to_text: default_text_to_text_priority_chain(),
        }
    }
}
