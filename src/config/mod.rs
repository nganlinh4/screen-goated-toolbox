//! Configuration module for screen-goated-toolbox.
//! 
//! This module is split into several sub-modules:
//! - `types`: Core types, enums, and helper functions
//! - `preset`: Preset struct definition
//! - `config_struct`: Config struct definition
//! - `defaults`: Config Default implementation
//! - `defaults_image`: Image-based default presets
//! - `defaults_text`: Text-based default presets
//! - `defaults_audio`: Audio and master default presets
//! - `io`: Config loading, saving, and language utilities

mod types;
mod preset;
mod config_struct;
mod defaults;
mod defaults_image;
mod defaults_text;
mod defaults_audio;
mod io;

// Re-export public types for external use
pub use types::{ThemeMode, Hotkey, ProcessingBlock, TtsLanguageCondition};
pub use preset::Preset;
pub use config_struct::Config;
pub use io::{load_config, save_config, get_all_languages};
