//! TTS (Text-to-Speech) related configuration types.

use serde::{Deserialize, Serialize};

// ============================================================================
// TTS METHOD
// ============================================================================

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub enum TtsMethod {
    #[default]
    GeminiLive, // Premium (Gemini Live)
    GoogleTranslate, // Fast (Google Translate)
    EdgeTTS,         // Good (Edge TTS)
}

// ============================================================================
// EDGE TTS VOICE CONFIG
// ============================================================================

/// Edge TTS voice configuration for a specific language
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EdgeTtsVoiceConfig {
    /// ISO 639-1 language code (e.g., "en", "vi", "ko")
    pub language_code: String,
    /// Human-readable language name
    pub language_name: String,
    /// Edge TTS voice name (e.g., "en-US-AriaNeural", "vi-VN-HoaiMyNeural")
    pub voice_name: String,
}

impl EdgeTtsVoiceConfig {
    pub fn new(language_code: &str, language_name: &str, voice_name: &str) -> Self {
        Self {
            language_code: language_code.to_string(),
            language_name: language_name.to_string(),
            voice_name: voice_name.to_string(),
        }
    }
}

// ============================================================================
// EDGE TTS SETTINGS
// ============================================================================

/// Edge TTS settings with pitch, rate, volume, and per-language voice configs
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EdgeTtsSettings {
    /// Pitch adjustment (-50 to +50 Hz, 0 = default)
    pub pitch: i32,
    /// Rate adjustment (-50 to +100 percent, 0 = default)
    pub rate: i32,
    /// Volume adjustment (-50 to +50 percent, 0 = default)
    pub volume: i32,
    /// Per-language voice configuration
    pub voice_configs: Vec<EdgeTtsVoiceConfig>,
}

impl Default for EdgeTtsSettings {
    fn default() -> Self {
        Self {
            pitch: 0,
            rate: 0,
            volume: 0,
            voice_configs: default_edge_tts_voice_configs(),
        }
    }
}

/// Default Edge TTS voice configurations for common languages
pub fn default_edge_tts_voice_configs() -> Vec<EdgeTtsVoiceConfig> {
    vec![
        EdgeTtsVoiceConfig::new("en", "English", "en-US-AriaNeural"),
        EdgeTtsVoiceConfig::new("vi", "Vietnamese", "vi-VN-HoaiMyNeural"),
        EdgeTtsVoiceConfig::new("ko", "Korean", "ko-KR-SunHiNeural"),
        EdgeTtsVoiceConfig::new("ja", "Japanese", "ja-JP-NanamiNeural"),
        EdgeTtsVoiceConfig::new("zh", "Chinese", "zh-CN-XiaoxiaoNeural"),
    ]
}

// ============================================================================
// TTS LANGUAGE CONDITION
// ============================================================================

/// A condition for TTS that applies a specific speaking instruction
/// when the detected language matches
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TtsLanguageCondition {
    /// ISO 639-3 language code from whatlang (e.g., "vie" for Vietnamese, "kor" for Korean)
    pub language_code: String,
    /// Human-readable language name for display
    pub language_name: String,
    /// The speaking instruction to apply when this language is detected
    pub instruction: String,
}

impl TtsLanguageCondition {
    pub fn new(language_code: &str, language_name: &str, instruction: &str) -> Self {
        Self {
            language_code: language_code.to_string(),
            language_name: language_name.to_string(),
            instruction: instruction.to_string(),
        }
    }
}

/// Default TTS language conditions
pub fn default_tts_language_conditions() -> Vec<TtsLanguageCondition> {
    vec![TtsLanguageCondition::new(
        "vie",
        "Vietnamese",
        "Speak in a \"giọng miền Tây\" accent.",
    )]
}
