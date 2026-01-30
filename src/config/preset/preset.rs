//! Preset definition with builder pattern for easy creation.
//!
//! A Preset represents a complete workflow configuration, containing:
//! - A chain of processing blocks
//! - Block connections (for graph-based workflows)
//! - Input/output behavior settings
//! - Hotkey bindings

use serde::{Deserialize, Serialize};

use super::block::ProcessingBlock;
use crate::config::types::Hotkey;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WindowGeometry {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

// ============================================================================
// PRESET STRUCT
// ============================================================================

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Preset {
    /// Unique identifier. Built-in presets start with "preset_"
    pub id: String,

    /// Display name
    pub name: String,

    /// Chain of processing blocks
    #[serde(default)]
    pub blocks: Vec<ProcessingBlock>,

    /// Graph connections: (from_block_idx, to_block_idx)
    /// Allows branching: one block can connect to multiple downstream blocks
    #[serde(default)]
    pub block_connections: Vec<(usize, usize)>,

    // -------------------------------------------------------------------------
    // Input Behavior
    // -------------------------------------------------------------------------
    /// Prompt mode: "fixed" or "dynamic" (user types custom prompt)
    #[serde(default = "default_prompt_mode")]
    pub prompt_mode: String,

    /// Type of preset: "image", "text", "audio", "video"
    #[serde(default = "default_preset_type")]
    pub preset_type: String,

    /// Text input mode: "select" (highlight text) or "type" (keyboard input)
    #[serde(default = "default_text_input_mode")]
    pub text_input_mode: String,

    /// Audio source: "mic" or "device"
    #[serde(default = "default_audio_source")]
    pub audio_source: String,

    /// Audio processing mode: "record_then_process" or "realtime"
    #[serde(default = "default_audio_processing_mode")]
    pub audio_processing_mode: String,

    /// Realtime window mode: "standard" (webview) or "minimal" (egui)
    #[serde(default = "default_realtime_window_mode")]
    pub realtime_window_mode: String,

    /// Video capture method
    #[serde(default)]
    pub video_capture_method: String,

    // -------------------------------------------------------------------------
    // Output Behavior
    // -------------------------------------------------------------------------
    /// Auto-paste result to active application
    #[serde(default)]
    pub auto_paste: bool,

    /// Add newline before pasting
    #[serde(default = "default_true")]
    pub auto_paste_newline: bool,

    // -------------------------------------------------------------------------
    // Audio Recording Options
    // -------------------------------------------------------------------------
    /// Hide the recording UI overlay
    #[serde(default)]
    pub hide_recording_ui: bool,

    /// Auto-stop recording on silence detection
    #[serde(default)]
    pub auto_stop_recording: bool,

    // -------------------------------------------------------------------------
    // Text Input Options
    // -------------------------------------------------------------------------
    /// Keep input window open after submit (for repeated inputs)
    #[serde(default)]
    pub continuous_input: bool,

    // -------------------------------------------------------------------------
    // Hotkeys
    // -------------------------------------------------------------------------
    /// Keyboard shortcuts to trigger this preset
    #[serde(default)]
    pub hotkeys: Vec<Hotkey>,

    // -------------------------------------------------------------------------
    // Special Flags
    // -------------------------------------------------------------------------
    /// Upcoming/preview feature flag
    #[serde(default)]
    pub is_upcoming: bool,

    /// MASTER preset: shows preset wheel for selection
    #[serde(default)]
    pub is_master: bool,

    /// Controller UI mode: hides advanced UI elements
    #[serde(default)]
    pub show_controller_ui: bool,

    /// Favorite preset for quick access via floating bubble
    #[serde(default)]
    pub is_favorite: bool,

    /// Saved window position and size for this preset
    #[serde(default)]
    pub window_geometry: Option<WindowGeometry>,
}

// ============================================================================
// DEFAULT VALUE FUNCTIONS
// ============================================================================

fn default_prompt_mode() -> String {
    "fixed".to_string()
}

fn default_preset_type() -> String {
    "image".to_string()
}

fn default_text_input_mode() -> String {
    "select".to_string()
}

fn default_audio_source() -> String {
    "mic".to_string()
}

fn default_audio_processing_mode() -> String {
    "record_then_process".to_string()
}

fn default_realtime_window_mode() -> String {
    "standard".to_string()
}

fn default_true() -> bool {
    true
}

// ============================================================================
// PRESET DEFAULT IMPL
// ============================================================================

impl Default for Preset {
    fn default() -> Self {
        Self {
            id: generate_preset_id(),
            name: "New Preset".to_string(),
            blocks: vec![ProcessingBlock::default()],
            block_connections: vec![],
            prompt_mode: "fixed".to_string(),
            preset_type: "image".to_string(),
            text_input_mode: "select".to_string(),
            audio_source: "mic".to_string(),
            audio_processing_mode: "record_then_process".to_string(),
            realtime_window_mode: "standard".to_string(),
            video_capture_method: "region".to_string(),
            auto_paste: false,
            auto_paste_newline: false,
            hide_recording_ui: false,
            auto_stop_recording: false,
            continuous_input: false,
            hotkeys: vec![],
            is_upcoming: false,
            is_master: false,
            show_controller_ui: false,
            is_favorite: false,
            window_geometry: None,
        }
    }
}

fn generate_preset_id() -> String {
    format!(
        "{:x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    )
}

// ============================================================================
// PRESET BUILDER
// ============================================================================

/// Builder for creating Presets with a fluent API.
///
/// # Example
/// ```
/// let preset = PresetBuilder::new("preset_translate", "Translate")
///     .image()
///     .blocks(vec![block1, block2])
///     .connections(vec![(0, 1)])
///     .build();
/// ```
#[derive(Clone)]
pub struct PresetBuilder {
    preset: Preset,
}

impl PresetBuilder {
    /// Create a new preset with ID and name
    pub fn new(id: &str, name: &str) -> Self {
        Self {
            preset: Preset {
                id: id.to_string(),
                name: name.to_string(),
                blocks: vec![],
                ..Default::default()
            },
        }
    }

    // -------------------------------------------------------------------------
    // Preset Type Setters
    // -------------------------------------------------------------------------

    /// Set as image preset
    pub fn image(mut self) -> Self {
        self.preset.preset_type = "image".to_string();
        self
    }

    /// Set as text preset with select input mode
    pub fn text_select(mut self) -> Self {
        self.preset.preset_type = "text".to_string();
        self.preset.text_input_mode = "select".to_string();
        self
    }

    /// Set as text preset with type input mode
    pub fn text_type(mut self) -> Self {
        self.preset.preset_type = "text".to_string();
        self.preset.text_input_mode = "type".to_string();
        self
    }

    /// Set as audio preset with mic source
    pub fn audio_mic(mut self) -> Self {
        self.preset.preset_type = "audio".to_string();
        self.preset.audio_source = "mic".to_string();
        self
    }

    /// Set as audio preset with device source
    pub fn audio_device(mut self) -> Self {
        self.preset.preset_type = "audio".to_string();
        self.preset.audio_source = "device".to_string();
        self
    }

    // -------------------------------------------------------------------------
    // Block Configuration
    // -------------------------------------------------------------------------

    /// Set the processing blocks
    pub fn blocks(mut self, blocks: Vec<ProcessingBlock>) -> Self {
        self.preset.blocks = blocks;
        self
    }

    /// Set block connections
    pub fn connections(mut self, connections: Vec<(usize, usize)>) -> Self {
        self.preset.block_connections = connections;
        self
    }

    // -------------------------------------------------------------------------
    // Output Behavior
    // -------------------------------------------------------------------------

    /// Enable auto-paste
    pub fn auto_paste(mut self) -> Self {
        self.preset.auto_paste = true;
        self
    }

    // -------------------------------------------------------------------------
    // Audio Options
    // -------------------------------------------------------------------------

    /// Enable auto-stop recording on silence
    pub fn auto_stop(mut self) -> Self {
        self.preset.auto_stop_recording = true;
        self
    }

    /// Enable realtime audio processing
    pub fn realtime(mut self) -> Self {
        self.preset.audio_processing_mode = "realtime".to_string();
        self
    }

    /// Set realtime window mode to minimal
    #[allow(dead_code)]
    pub fn minimal_mode(mut self) -> Self {
        self.preset.realtime_window_mode = "minimal".to_string();
        self
    }

    // -------------------------------------------------------------------------
    // Text Options
    // -------------------------------------------------------------------------

    /// Enable continuous input mode
    pub fn continuous(mut self) -> Self {
        self.preset.continuous_input = true;
        self
    }

    /// Enable dynamic prompt mode (user types custom prompt)
    pub fn dynamic_prompt(mut self) -> Self {
        self.preset.prompt_mode = "dynamic".to_string();
        self
    }

    // -------------------------------------------------------------------------
    // Special Flags
    // -------------------------------------------------------------------------

    /// Mark as MASTER preset
    pub fn master(mut self) -> Self {
        self.preset.is_master = true;
        self.preset.show_controller_ui = true;
        self.preset.blocks = vec![]; // MASTER presets don't have blocks
        self
    }

    // -------------------------------------------------------------------------
    // Build
    // -------------------------------------------------------------------------

    /// Build the final Preset
    pub fn build(self) -> Preset {
        self.preset
    }
}

// ============================================================================
// PRESET HELPER METHODS
// ============================================================================

impl Preset {
    /// Check if this is a built-in preset (ID starts with "preset_")
    pub fn is_builtin(&self) -> bool {
        self.id.starts_with("preset_")
    }

    /// Check if this is a MASTER preset
    pub fn is_master_preset(&self) -> bool {
        self.is_master
    }

    /// Get the first block (input block)
    pub fn input_block(&self) -> Option<&ProcessingBlock> {
        self.blocks.first()
    }

    /// Get mutable reference to the first block
    pub fn input_block_mut(&mut self) -> Option<&mut ProcessingBlock> {
        self.blocks.first_mut()
    }
}
