//! Configuration types module.
//!
//! This module organizes configuration-related types into logical groups:
//! - `translation_gummy`: Settings for the Translation Gummy mini app
//! - `enums`: Core enums (ThemeMode)
//! - `hotkey`: Hotkey binding type
//! - `model_priority`: Smart retry priority chains
//! - `tts`: TTS-related types (TtsMethod, EdgeTtsSettings, etc.)

mod custom_models;
mod enums;
mod hotkey;
mod model_priority;
mod profile;
mod translation_gummy;
mod tts;

// Re-export all types for easy access
pub use translation_gummy::TranslationGummySettings;

pub use custom_models::{CustomModelDefinition, CustomModelType};

pub use enums::{
    DEFAULT_HISTORY_LIMIT, DEFAULT_PROJECTS_LIMIT, ThemeMode, get_system_ui_language,
};

pub use hotkey::Hotkey;

pub use model_priority::ModelPriorityChains;

pub use profile::PresetProfile;

pub use tts::{
    EdgeTtsSettings, EdgeTtsVoiceConfig, KokoroSettings, KokoroVoiceConfig, MagpieSettings,
    MagpieVoiceConfig, StepAudioReferenceVoice, StepAudioSettings, StepAudioVoiceConfig,
    SupertonicSettings, SupertonicVoiceConfig, TtsLanguageCondition, TtsMethod, TtsPlaygroundMode,
    TtsPlaygroundSettings, VieneuSettings, VoxtralSettings, default_tts_language_conditions,
    step_audio_tts_text_issue,
};
