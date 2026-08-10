pub(crate) mod audio;
pub(crate) mod client;
#[path = "../../../../src/api/gemini_live/mod.rs"]
pub(crate) mod gemini_live;
pub(crate) mod groq;
pub(crate) mod ollama;
#[path = "../../../../src/api/provider_credentials.rs"]
pub(crate) mod provider_credentials;
pub(crate) mod realtime_audio;
#[path = "../../../../src/api/taalas.rs"]
pub(crate) mod taalas;
#[path = "../../../../src/api/tts/mod.rs"]
pub(crate) mod tts;

pub(crate) const WIPE_SIGNAL: &str = "\x00WIPE\x00";

pub(crate) fn gemini_thinking_config(model: &str) -> Option<serde_json::Value> {
    match crate::model_config::ordinary_reasoning_policy("google", model) {
        crate::model_config::OrdinaryReasoningPolicy::GeminiBudget(budget) => {
            Some(serde_json::json!({ "thinkingBudget": budget }))
        }
        crate::model_config::OrdinaryReasoningPolicy::GeminiLevel(level) => {
            Some(serde_json::json!({ "thinkingLevel": level }))
        }
        _ => None,
    }
}

pub(crate) fn apply_ordinary_openai_reasoning_policy(
    payload: &mut serde_json::Value,
    provider: &str,
    model: &str,
) {
    if let crate::model_config::OrdinaryReasoningPolicy::OpenAiEffort(effort) =
        crate::model_config::ordinary_reasoning_policy(provider, model)
    {
        payload["reasoning_effort"] = serde_json::Value::String(effort.to_string());
    }
}

pub(crate) fn apply_ordinary_openrouter_reasoning_policy(
    payload: &mut serde_json::Value,
    model: &str,
) {
    if let crate::model_config::OrdinaryReasoningPolicy::OpenAiEffort(effort) =
        crate::model_config::ordinary_reasoning_policy("openrouter", model)
    {
        payload["reasoning"] = serde_json::json!({ "effort": effort });
    }
}
