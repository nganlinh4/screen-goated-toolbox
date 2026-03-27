pub mod audio;
pub mod client;
pub mod gemini_live;
pub mod ollama;
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

/// Returns an explicit Gemini thinking configuration when the model needs one.
pub fn gemini_thinking_config(model: &str) -> Option<serde_json::Value> {
    if model.contains("gemini-3.1-flash-lite") {
        return Some(serde_json::json!({
            "thinkingLevel": "MINIMAL"
        }));
    }

    let supports_thinking = (model.contains("gemini-2.5-flash") && !model.contains("lite"))
        || model.contains("gemini-3-flash-preview")
        || model.contains("gemini-robotics");

    supports_thinking.then(|| {
        serde_json::json!({
            "includeThoughts": true
        })
    })
}

#[cfg(test)]
mod tests {
    use super::gemini_thinking_config;

    #[test]
    fn disables_thinking_for_gemini_3_1_flash_lite() {
        let config = gemini_thinking_config("gemini-3.1-flash-lite-preview")
            .expect("3.1 flash lite should get explicit thinking config");

        assert_eq!(
            config.get("thinkingLevel").and_then(|v| v.as_str()),
            Some("MINIMAL")
        );
        assert!(config.get("includeThoughts").is_none());
    }

    #[test]
    fn enables_thought_streaming_for_supported_gemini_models() {
        let config = gemini_thinking_config("gemini-2.5-flash")
            .expect("2.5 flash should get explicit thought streaming config");

        assert_eq!(
            config.get("includeThoughts").and_then(|v| v.as_bool()),
            Some(true)
        );
    }
}
