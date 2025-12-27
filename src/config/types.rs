//! Core types and enums for configuration.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// --- CONSTANTS ---
pub const DEFAULT_HISTORY_LIMIT: usize = 50;

// --- THEME MODE ENUM ---
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum ThemeMode {
    System,
    Dark,
    Light,
}

// --- TTS METHOD ENUM ---
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum TtsMethod {
    GeminiLive,      // Xịn (Gemini Live)
    GoogleTranslate, // Nhanh (Google Translate)
    EdgeTTS,         // Tốt (Edge TTS)
}

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

/// Edge TTS settings
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

pub fn default_edge_tts_voice_configs() -> Vec<EdgeTtsVoiceConfig> {
    vec![
        EdgeTtsVoiceConfig {
            language_code: "en".to_string(),
            language_name: "English".to_string(),
            voice_name: "en-US-AriaNeural".to_string(),
        },
        EdgeTtsVoiceConfig {
            language_code: "vi".to_string(),
            language_name: "Vietnamese".to_string(),
            voice_name: "vi-VN-HoaiMyNeural".to_string(),
        },
        EdgeTtsVoiceConfig {
            language_code: "ko".to_string(),
            language_name: "Korean".to_string(),
            voice_name: "ko-KR-SunHiNeural".to_string(),
        },
        EdgeTtsVoiceConfig {
            language_code: "ja".to_string(),
            language_name: "Japanese".to_string(),
            voice_name: "ja-JP-NanamiNeural".to_string(),
        },
        EdgeTtsVoiceConfig {
            language_code: "zh".to_string(),
            language_name: "Chinese".to_string(),
            voice_name: "zh-CN-XiaoxiaoNeural".to_string(),
        },
    ]
}

pub fn default_edge_tts_settings() -> EdgeTtsSettings {
    EdgeTtsSettings::default()
}

pub fn get_system_ui_language() -> String {
    let sys_locale = sys_locale::get_locale().unwrap_or_default();
    let lang_code = sys_locale.split('-').next().unwrap_or("en").to_lowercase();

    match lang_code.as_str() {
        "vi" => "vi".to_string(),
        "ko" => "ko".to_string(),
        "en" => "en".to_string(),
        _ => "en".to_string(), // Default to English for unsupported languages
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Hotkey {
    pub code: u32,
    pub name: String,
    pub modifiers: u32,
}

// --- NEW: PROCESSING BLOCK ---
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ProcessingBlock {
    #[serde(default = "generate_id")]
    pub id: String,
    pub block_type: String, // "image", "audio", "text"
    pub model: String,
    pub prompt: String,
    pub selected_language: String, // Context var {language1}
    #[serde(default)]
    pub language_vars: HashMap<String, String>, // Context vars {language1}, etc.
    pub streaming_enabled: bool,
    #[serde(default = "default_render_mode")]
    pub render_mode: String, // "stream", "plain", "markdown"

    // UI Behavior
    #[serde(default = "default_true")]
    pub show_overlay: bool,
    #[serde(default)]
    pub auto_copy: bool, // Only one block in chain should have this true
    #[serde(default)]
    pub auto_speak: bool,
}

pub fn default_true() -> bool {
    true
}
pub fn default_render_mode() -> String {
    "stream".to_string()
}
pub fn generate_id() -> String {
    format!(
        "{:x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    )
}

impl Default for ProcessingBlock {
    fn default() -> Self {
        Self {
            id: generate_id(),
            block_type: "text".to_string(),
            model: "text_accurate_kimi".to_string(),
            prompt: "Translate to {language1}. Output ONLY the translation.".to_string(),
            selected_language: "Vietnamese".to_string(),
            language_vars: HashMap::new(),
            streaming_enabled: true,
            render_mode: "stream".to_string(),
            show_overlay: true,
            auto_copy: false,
            auto_speak: false,
        }
    }
}

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

pub fn default_tts_language_conditions() -> Vec<TtsLanguageCondition> {
    vec![TtsLanguageCondition {
        language_code: "vie".to_string(),
        language_name: "Vietnamese".to_string(),
        instruction: "Speak in a \"giọng miền Tây\" accent.".to_string(),
    }]
}

// --- Default Function Helpers ---
pub fn default_preset_type() -> String {
    "image".to_string()
}
pub fn default_audio_source() -> String {
    "mic".to_string()
}
pub fn default_prompt_mode() -> String {
    "fixed".to_string()
}
pub fn default_text_input_mode() -> String {
    "select".to_string()
}
pub fn default_theme_mode() -> ThemeMode {
    ThemeMode::System
}
pub fn default_auto_paste_newline() -> bool {
    true
}
pub fn default_history_limit() -> usize {
    DEFAULT_HISTORY_LIMIT
}
pub fn default_graphics_mode() -> String {
    "standard".to_string()
}
pub fn default_audio_processing_mode() -> String {
    "record_then_process".to_string()
}
pub fn default_tts_voice() -> String {
    "Aoede".to_string()
}
pub fn default_tts_speed() -> String {
    "Fast".to_string()
}
pub fn default_realtime_translation_model() -> String {
    "groq-llama".to_string()
}
pub fn default_realtime_font_size() -> u32 {
    16
}
pub fn default_realtime_window_size() -> (i32, i32) {
    (500, 180)
}
pub fn default_realtime_target_language() -> String {
    "Vietnamese".to_string()
}
pub fn default_ollama_base_url() -> String {
    "http://localhost:11434".to_string()
}
pub fn default_tts_method() -> TtsMethod {
    TtsMethod::GeminiLive
}
