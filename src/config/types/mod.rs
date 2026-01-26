//! Configuration types module.
//!
//! This module organizes all configuration-related types into logical groups:
//! - `enums`: Core enums (ThemeMode, BlockType)
//! - `hotkey`: Hotkey binding type
//! - `tts`: TTS-related types (TtsMethod, EdgeTtsSettings, etc.)

mod enums;
mod hotkey;
mod tts;

// Re-export all types for easy access
pub use enums::{
    get_system_ui_language, BlockType, ThemeMode, DEFAULT_HISTORY_LIMIT, DEFAULT_PROJECTS_LIMIT,
};

pub use hotkey::Hotkey;

pub use tts::{
    default_tts_language_conditions, EdgeTtsSettings, EdgeTtsVoiceConfig, TtsLanguageCondition,
    TtsMethod,
};
