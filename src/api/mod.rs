pub mod audio;
pub mod client;
pub mod gemini_embed;
pub mod gemini_generate;
pub mod gemini_live;
mod gemini_schema;
pub mod groq;
pub mod ollama;
pub mod openai_compat;
pub(crate) mod provider_credentials;
pub mod providers;
pub mod realtime_audio;
pub mod taalas;
pub mod text;
pub mod tts;
pub mod types;
pub mod vision;

pub use audio::{record_and_stream_gemini_live, record_audio_and_transcribe};
pub use text::{
    RefineTextRequest, TranslateTextRequest, refine_text_streaming, translate_text_streaming,
};
pub use vision::{TranslateImageRequest, translate_image_streaming};
// realtime_audio types/functions are used directly where needed via crate::api::realtime_audio::

/// Special prefix signal that tells callbacks to clear their accumulator before processing
/// When a chunk starts with this, the callback should: 1) Clear acc 2) Add the content after this prefix
pub const WIPE_SIGNAL: &str = "\x00WIPE\x00";

/// Lowest-latency thinking policy for ordinary model calls.
pub fn gemini_thinking_config(model: &str) -> Option<serde_json::Value> {
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

/// Deliberate low-thinking override for correctness-sensitive interactive tasks.
///
/// Never returns `None` for an endpoint that owns a thinking policy. Callers
/// attach this config only when it is `Some`, so returning `None` would send no
/// `thinkingConfig` at all and leave the provider free to apply its own default
/// — the opposite of the catalog's intent. Budget-policy endpoints therefore
/// keep their floor rather than being raised to a level they do not express.
pub fn gemini_important_task_thinking_config(model: &str) -> Option<serde_json::Value> {
    match crate::model_config::ordinary_reasoning_policy("google", model) {
        crate::model_config::OrdinaryReasoningPolicy::GeminiLevel(_) => {
            Some(serde_json::json!({ "thinkingLevel": "LOW" }))
        }
        crate::model_config::OrdinaryReasoningPolicy::GeminiBudget(budget) => {
            Some(serde_json::json!({ "thinkingBudget": budget }))
        }
        _ => None,
    }
}

/// Apply the catalog-owned lowest supported reasoning effort to ordinary
/// OpenAI-compatible requests.
pub fn apply_ordinary_openai_reasoning_policy(
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

/// Apply an exact catalog reasoning policy using OpenRouter's nested request
/// shape. OpenRouter's top-level `reasoning_effort` is not equivalent.
pub fn apply_ordinary_openrouter_reasoning_policy(payload: &mut serde_json::Value, model: &str) {
    if let crate::model_config::OrdinaryReasoningPolicy::OpenAiEffort(effort) =
        crate::model_config::ordinary_reasoning_policy("openrouter", model)
    {
        payload["reasoning"] = serde_json::json!({ "effort": effort });
    }
}

#[cfg(test)]
mod tests {
    use super::{
        apply_ordinary_openai_reasoning_policy, apply_ordinary_openrouter_reasoning_policy,
        gemini_thinking_config,
    };

    #[test]
    fn minimizes_thinking_for_flash_lite_models() {
        for model in ["gemini-3.1-flash-lite", "gemini-3.5-flash-lite"] {
            let config = gemini_thinking_config(model)
                .expect("Flash-Lite should get explicit thinking config");

            assert_eq!(
                config.get("thinkingLevel").and_then(|v| v.as_str()),
                Some("MINIMAL")
            );
            assert!(config.get("includeThoughts").is_none());
        }
    }

    #[test]
    fn disables_thinking_for_gemma_4_models() {
        let config = gemini_thinking_config("gemma-4-26b-a4b-it")
            .expect("gemma 4 should get explicit minimal thinking config");

        assert_eq!(
            config.get("thinkingLevel").and_then(|v| v.as_str()),
            Some("MINIMAL")
        );
        assert!(config.get("includeThoughts").is_none());
    }

    #[test]
    fn important_tasks_use_low_thinking_without_exposing_thoughts() {
        let config = super::gemini_important_task_thinking_config("gemini-3.5-flash-lite")
            .expect("Gemini 3 important task config");
        assert_eq!(config["thinkingLevel"], "LOW");
        assert!(config.get("includeThoughts").is_none());
    }

    #[test]
    fn openai_compatible_reasoning_comes_from_exact_catalog_profiles() {
        for (provider, model, expected) in [
            ("groq", "qwen/qwen3.6-27b", "none"),
            ("groq", "openai/gpt-oss-120b", "low"),
            ("groq", "openai/gpt-oss-20b", "low"),
        ] {
            let mut payload = serde_json::json!({});
            apply_ordinary_openai_reasoning_policy(&mut payload, provider, model);
            assert_eq!(payload["reasoning_effort"], expected, "{model}");
        }
    }

    #[test]
    fn budget_policy_models_keep_their_floor_on_important_tasks() {
        // A budget-policy endpoint must still receive an explicit config here.
        // Returning None would attach no thinkingConfig and hand the decision
        // back to the provider default.
        let config = super::gemini_important_task_thinking_config("gemini-robotics-er-2-preview")
            .expect("budget-policy models must still get an explicit config");
        assert_eq!(config["thinkingBudget"], 0);
        assert!(config.get("thinkingLevel").is_none());
    }

    #[test]
    fn openrouter_reasoning_uses_its_nested_wire_contract() {
        let mut payload = serde_json::json!({});
        apply_ordinary_openrouter_reasoning_policy(
            &mut payload,
            "nvidia/nemotron-3-super-120b-a12b:free",
        );

        assert_eq!(payload["reasoning"]["effort"], "none");
        assert!(payload.get("reasoning_effort").is_none());
    }
}
