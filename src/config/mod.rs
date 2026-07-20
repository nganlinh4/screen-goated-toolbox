//! Configuration module for screen-goated-toolbox.
//!
//! This module provides a comprehensive, organized configuration system:
//!
//! ## Structure
//! - `config`: Main Config struct
//! - `preset`: Preset and ProcessingBlock with builder patterns
//! - `types`: Core types (enums, TTS settings, hotkeys)
//! - `io`: Load/save operations
//!
//! ## Usage
//! ```rust
//! use crate::config::{Config, Preset, ProcessingBlock, load_config, save_config};
//!
//! // Load config from disk
//! let config = load_config();
//!
//! // Create a new preset using the builder pattern
//! use crate::config::preset::{PresetBuilder, BlockBuilder};
//! let preset = PresetBuilder::new("my_preset", "My Preset")
//!     .image()
//!     .blocks(vec![
//!         BlockBuilder::image(crate::model_config::DEFAULT_IMAGE_MODEL_ID)
//!             .prompt("Extract text.")
//!             .language("Vietnamese")
//!             .build()
//!     ])
//!     .build();
//! ```

#[allow(clippy::module_inception)]
mod config;
mod io;
pub mod preset;
pub mod tts_catalog;
mod tts_catalog_gemini;
pub mod types;

// ============================================================================
// RE-EXPORTS - Primary API
// ============================================================================

// Config struct and structured hotkey conflict ownership
pub use config::{Config, GlobalHotkeyOwner, HotkeyConflict};

// Preset and ProcessingBlock
pub use preset::{Preset, ProcessingBlock};

// I/O functions
pub use io::{get_all_languages, load_config, save_config};

// ============================================================================
// RE-EXPORTS - Types (only what's actually used externally)
// ============================================================================

// Core enums
pub use types::ThemeMode;

// Hotkey
pub use types::Hotkey;

// Retry priority types
pub use types::ModelPriorityChains;

// TTS types
pub use types::{
    EdgeTtsSettings, EdgeTtsVoiceConfig, KokoroSettings, KokoroVoiceConfig, MagpieSettings,
    MagpieVoiceConfig, StepAudioReferenceVoice, StepAudioSettings, StepAudioVoiceConfig,
    SupertonicSettings, SupertonicVoiceConfig, TtsLanguageCondition, TtsMethod, TtsPlaygroundMode,
    TtsPlaygroundSettings, VieneuSettings, step_audio_tts_text_issue,
};

// Translation Gummy
pub use types::TranslationGummySettings;
