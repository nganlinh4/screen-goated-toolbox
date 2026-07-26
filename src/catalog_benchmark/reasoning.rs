use crate::model_config::{ModelConfig, OrdinaryReasoningPolicy};

pub(super) fn reasoning_policy_label(model: &ModelConfig) -> String {
    match crate::model_config::ordinary_reasoning_policy(&model.provider, &model.full_name) {
        OrdinaryReasoningPolicy::NotApplicable => "not-applicable".to_string(),
        OrdinaryReasoningPolicy::GeminiBudget(budget) => format!("gemini-budget:{budget}"),
        OrdinaryReasoningPolicy::GeminiLevel(level) => {
            format!("gemini-level:{}", level.to_ascii_lowercase())
        }
        OrdinaryReasoningPolicy::OpenAiEffort(effort) => format!("openai-effort:{effort}"),
        OrdinaryReasoningPolicy::ProviderManaged => "provider-managed".to_string(),
        OrdinaryReasoningPolicy::LiveProfile => "live-profile".to_string(),
    }
}
