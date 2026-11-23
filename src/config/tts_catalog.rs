//! Shared TTS provider metadata.

#[cfg(feature = "recorder-worker")]
use crate::config::TtsMethod;

pub use super::tts_catalog_gemini::{GEMINI_VOICES, SUPPORTED_GEMINI_INSTRUCTION_LANGUAGES};

#[derive(Clone, Debug)]
#[cfg(feature = "recorder-worker")]
pub struct TtsProviderInfo {
    pub method: TtsMethod,
    pub id: &'static str,
    pub label: &'static str,
    pub narration_supported: bool,
}

#[cfg(feature = "recorder-worker")]
pub const TTS_PROVIDERS: &[TtsProviderInfo] = &[
    TtsProviderInfo {
        method: TtsMethod::GeminiLive,
        id: "GeminiLive",
        label: "Gemini Live",
        narration_supported: true,
    },
    TtsProviderInfo {
        method: TtsMethod::EdgeTTS,
        id: "EdgeTTS",
        label: "Edge TTS",
        narration_supported: true,
    },
    TtsProviderInfo {
        method: TtsMethod::GoogleTranslate,
        id: "GoogleTranslate",
        label: "Google Translate",
        narration_supported: true,
    },
];

#[cfg(feature = "recorder-worker")]
pub fn tts_method_id(method: &TtsMethod) -> &'static str {
    TTS_PROVIDERS
        .iter()
        .find(|provider| &provider.method == method)
        .map(|provider| provider.id)
        .unwrap_or("GeminiLive")
}

#[cfg(feature = "recorder-worker")]
pub fn narration_tts_providers() -> impl Iterator<Item = &'static TtsProviderInfo> {
    TTS_PROVIDERS
        .iter()
        .filter(|provider| provider.narration_supported)
}
