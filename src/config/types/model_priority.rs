use serde::{Deserialize, Serialize};

fn default_image_to_text_priority_chain() -> Vec<String> {
    crate::model_config::default_image_to_text_priority_chain_ids()
        .iter()
        .map(|id| (*id).to_string())
        .collect()
}

fn default_text_to_text_priority_chain() -> Vec<String> {
    crate::model_config::default_text_to_text_priority_chain_ids()
        .iter()
        .map(|id| (*id).to_string())
        .collect()
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
