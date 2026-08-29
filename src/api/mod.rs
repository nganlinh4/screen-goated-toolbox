pub mod audio;
pub mod client;
pub mod gemini_embed;
pub mod gemini_generate;
pub mod gemini_live;
mod gemini_schema;
pub(crate) mod gemini_transcribe;
pub mod groq;
pub mod ollama;
pub mod openai_compat;
mod output_normalization;
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
/// NVIDIA NIM speaks the OpenAI chat-completions dialect at this endpoint.
pub const NVIDIA_CHAT_COMPLETIONS_URL: &str =
    "https://integrate.api.nvidia.com/v1/chat/completions";

pub const WIPE_SIGNAL: &str = "\x00WIPE\x00";

/// Whether an endpoint exposes incremental response bytes that can drive the
/// shared progress-idle watchdog. This is a transport capability, independent
/// of whether the caller presents partial output.
pub fn endpoint_supports_progress_streaming(provider: &str, api_model: &str) -> bool {
    !(matches!(provider, "google-gtx" | "taalas")
        || provider == "groq" && api_model.starts_with("groq/compound"))
}

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

/// Apply the lowest supported reasoning effort to ordinary OpenAI-compatible
/// requests.
///
/// The availability feed wins over the catalog when it has an opinion. Both
/// describe the same thing -- how to ask this endpoint to stop thinking -- but
/// the catalog fixes its answer when the build is cut, while the monitor
/// rediscovers it from the live endpoint every couple of hours. A control an
/// endpoint no longer accepts is not a cosmetic mismatch: during evaluation it
/// turned a healthy model into HTTP 500.
pub fn apply_ordinary_openai_reasoning_policy(
    payload: &mut serde_json::Value,
    provider: &str,
    model: &str,
) {
    #[cfg(not(feature = "recorder-worker"))]
    if let Some(control) = crate::model_feed::store::control_for(provider, model) {
        apply_feed_reasoning_control(payload, control);
        return;
    }
    if let crate::model_config::OrdinaryReasoningPolicy::OpenAiEffort(effort) =
        crate::model_config::ordinary_reasoning_policy(provider, model)
    {
        payload["reasoning_effort"] = serde_json::Value::String(effort.to_string());
    }
}

/// Shape one request the way the publisher proved the endpoint accepts.
///
/// The typed labels mirror the monitor's versioned contract in
/// `scripts/monitor_nvidia_models.py`; an unknown label invalidates the feed
/// rather than being guessed at on a user request.
#[cfg(not(feature = "recorder-worker"))]
fn apply_feed_reasoning_control(
    payload: &mut serde_json::Value,
    control: crate::model_feed::FeedControl,
) {
    use crate::model_feed::FeedControl;
    match control {
        FeedControl::EffortNone => {
            payload["reasoning_effort"] = serde_json::Value::String("none".into());
        }
        FeedControl::EffortLow => {
            payload["reasoning_effort"] = serde_json::Value::String("low".into());
        }
        FeedControl::TemplateKwargs => {
            payload["chat_template_kwargs"] = serde_json::json!({ "thinking": false });
        }
        FeedControl::NoThink => prepend_system_message(payload, "/no_think"),
        FeedControl::ThinkingOff => prepend_system_message(payload, "detailed thinking off"),
        FeedControl::Plain => {}
    }
}

/// Puts a control instruction ahead of the conversation, as the publisher sends it.
#[cfg(not(feature = "recorder-worker"))]
fn prepend_system_message(payload: &mut serde_json::Value, content: &str) {
    let Some(messages) = payload["messages"].as_array_mut() else {
        return;
    };
    if let Some(existing) = messages
        .first_mut()
        .filter(|message| message["role"] == "system")
    {
        if let Some(text) = existing["content"].as_str() {
            existing["content"] = serde_json::Value::String(format!("{content}\n\n{text}"));
            return;
        }
        if let Some(parts) = existing["content"].as_array_mut() {
            parts.insert(0, serde_json::json!({ "type": "text", "text": content }));
            return;
        }
    }
    messages.insert(
        0,
        serde_json::json!({ "role": "system", "content": content }),
    );
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
    fn a_published_control_is_sent_exactly_as_the_publisher_proved_it() {
        use super::apply_feed_reasoning_control;
        use crate::model_feed::FeedControl;

        let mut payload = serde_json::json!({ "messages": [{"role": "user", "content": "hi"}] });
        apply_feed_reasoning_control(&mut payload, FeedControl::EffortNone);
        assert_eq!(payload["reasoning_effort"], "none");

        let mut payload = serde_json::json!({ "messages": [] });
        apply_feed_reasoning_control(&mut payload, FeedControl::TemplateKwargs);
        assert_eq!(payload["chat_template_kwargs"]["thinking"], false);

        // The system-message controls have to reach the conversation, not the
        // top level, or the endpoint simply thinks anyway.
        let mut payload = serde_json::json!({ "messages": [{"role": "user", "content": "hi"}] });
        apply_feed_reasoning_control(&mut payload, FeedControl::ThinkingOff);
        assert_eq!(payload["messages"][0]["role"], "system");
        assert_eq!(payload["messages"][0]["content"], "detailed thinking off");
        assert_eq!(payload["messages"][1]["role"], "user");
    }

    #[test]
    fn a_plain_control_leaves_the_request_untouched() {
        use super::apply_feed_reasoning_control;
        use crate::model_feed::FeedControl;

        let original = serde_json::json!({ "messages": [{"role": "user", "content": "hi"}] });
        let mut payload = original.clone();
        apply_feed_reasoning_control(&mut payload, FeedControl::Plain);
        assert_eq!(payload, original);
    }

    #[test]
    fn a_system_control_preserves_the_existing_system_instruction() {
        use super::apply_feed_reasoning_control;
        use crate::model_feed::FeedControl;

        let mut payload = serde_json::json!({
            "messages": [
                {"role": "system", "content": "Translate faithfully."},
                {"role": "user", "content": "hi"}
            ]
        });
        apply_feed_reasoning_control(&mut payload, FeedControl::NoThink);

        assert_eq!(payload["messages"].as_array().unwrap().len(), 2);
        assert_eq!(
            payload["messages"][0]["content"],
            "/no_think\n\nTranslate faithfully."
        );
    }

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
            ("groq", "qwen/qwen3.8-27b", "none"),
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

    #[test]
    fn progress_streaming_is_transport_capability_not_presentation_policy() {
        assert!(super::endpoint_supports_progress_streaming(
            "google",
            "gemini-3.5-flash-lite"
        ));
        assert!(super::endpoint_supports_progress_streaming(
            "nvidia",
            "nvidia/nemotron-3.5-lightning-30b-a3b"
        ));
        assert!(!super::endpoint_supports_progress_streaming(
            "groq",
            "groq/compound-mini"
        ));
        assert!(!super::endpoint_supports_progress_streaming(
            "google-gtx",
            "translate.googleapis.com/gtx"
        ));
        assert!(!super::endpoint_supports_progress_streaming(
            "taalas",
            "llama-3.1-8b"
        ));
    }
}
