pub mod audio;
pub mod client;
pub mod gemini_embed;
pub mod gemini_generate;
pub mod gemini_live;
mod gemini_schema;
pub(crate) mod gemini_transcribe;
pub mod groq;
#[cfg(test)]
pub(crate) mod http_test_support;
pub mod ollama;
pub mod openai_compat;
mod output_normalization;
pub(crate) mod provider_credentials;
pub mod providers;
pub mod realtime_audio;
mod reasoning_policy;
pub mod taalas;
pub mod text;
pub mod tts;
pub mod vision;

pub(crate) use audio::record_and_stream_gemini_live;
pub use audio::record_audio_and_transcribe;
pub use reasoning_policy::{
    apply_ordinary_openai_reasoning_policy, apply_ordinary_openrouter_reasoning_policy,
    gemini_important_task_thinking_config, gemini_thinking_config,
};
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

#[cfg(test)]
mod tests {
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
