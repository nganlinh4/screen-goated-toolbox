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
//!         BlockBuilder::image("maverick")
//!             .prompt("Extract text.")
//!             .language("Vietnamese")
//!             .build()
//!     ])
//!     .build();
//! ```

mod config;
mod io;
pub mod preset;
pub mod types;

// ============================================================================
// RE-EXPORTS - Primary API
// ============================================================================

// Config struct
pub use config::Config;

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

// TTS types
pub use types::{EdgeTtsSettings, EdgeTtsVoiceConfig, TtsLanguageCondition, TtsMethod};
