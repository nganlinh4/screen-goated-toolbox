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
mod languages;
#[cfg(not(feature = "recorder-worker"))]
mod live_translate_sync;
pub mod preset;
pub mod speech_languages;
pub mod tts_catalog;
mod tts_catalog_gemini;
pub mod types;

// ============================================================================
// RE-EXPORTS - Primary API
// ============================================================================

// Config struct and structured hotkey conflict ownership
pub use config::Config;
#[cfg(not(feature = "recorder-worker"))]
pub use config::{GlobalHotkeyOwner, HotkeyConflict};

// Preset and ProcessingBlock
#[cfg(not(feature = "recorder-worker"))]
pub use preset::Preset;
#[cfg(not(feature = "recorder-worker"))]
pub use preset::ProcessingBlock;

// I/O functions
pub use io::load_config;
#[cfg(not(feature = "recorder-worker"))]
pub(crate) use io::try_save_current_config;
#[cfg(not(feature = "recorder-worker"))]
pub(crate) use io::unread_config_path;
#[cfg(not(feature = "recorder-worker"))]
pub use io::{save_config, try_save_config};
#[cfg(not(feature = "recorder-worker"))]
pub use languages::get_all_languages;
pub use languages::language_from_name;

// ============================================================================
// RE-EXPORTS - Types (only what's actually used externally)
// ============================================================================

// Core enums
#[cfg(not(feature = "recorder-worker"))]
pub use types::ThemeMode;

// Hotkey
pub use types::Hotkey;

// Retry priority types
#[cfg(not(feature = "recorder-worker"))]
pub use types::ModelPriorityChains;

// Selective restore-defaults dialog state
#[cfg(not(feature = "recorder-worker"))]
pub use types::RestoreDefaultsSelection;

// TTS types
#[cfg(not(feature = "recorder-worker"))]
pub use types::TtsPlaygroundMode;
pub use types::{
    EdgeTtsSettings, EdgeTtsVoiceConfig, TtsLanguageCondition, TtsMethod, TtsPlaygroundSettings,
};

// Translation Gummy
pub use types::TranslationGummySettings;
