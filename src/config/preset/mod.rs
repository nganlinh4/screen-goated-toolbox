//! Preset module - Preset and ProcessingBlock definitions with builders.
//!
//! This module provides:
//! - `ProcessingBlock`: A single processing step in a chain
//! - `BlockBuilder`: Fluent API for creating blocks
//! - `Preset`: A complete workflow configuration
//! - `PresetBuilder`: Fluent API for creating presets
//! - `defaults`: Built-in preset definitions

mod block;
pub mod defaults;
mod preset;

pub use block::{BlockBuilder, ProcessingBlock};
pub use preset::{Preset, PresetBuilder, WindowGeometry};

// Re-export default preset functions for convenience
pub use defaults::get_default_presets;
