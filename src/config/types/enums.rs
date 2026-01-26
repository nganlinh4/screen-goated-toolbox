//! Core enums and constants for configuration.

use serde::{Deserialize, Serialize};

// ============================================================================
// CONSTANTS
// ============================================================================

pub const DEFAULT_HISTORY_LIMIT: usize = 50;
pub const DEFAULT_PROJECTS_LIMIT: usize = 50;

// ============================================================================
// THEME MODE
// ============================================================================

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub enum ThemeMode {
    #[default]
    System,
    Dark,
    Light,
}

// ============================================================================
// BLOCK TYPE - Used by ProcessingBlock for type checking
// ============================================================================

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum BlockType {
    InputAdapter, // Pass-through node for input
    Image,
    #[default]
    Text,
    Audio,
}

impl BlockType {
    pub fn from_str(s: &str) -> Self {
        match s {
            "input_adapter" => BlockType::InputAdapter,
            "image" => BlockType::Image,
            "audio" => BlockType::Audio,
            _ => BlockType::Text,
        }
    }
}

// ============================================================================
// UTILITY FUNCTIONS
// ============================================================================

/// Get system UI language (vi, ko, or en)
pub fn get_system_ui_language() -> String {
    let sys_locale = sys_locale::get_locale().unwrap_or_default();
    let lang_code = sys_locale.split('-').next().unwrap_or("en").to_lowercase();

    match lang_code.as_str() {
        "vi" => "vi".to_string(),
        "ko" => "ko".to_string(),
        "ja" => "ja".to_string(),
        "zh" => "zh".to_string(),
        _ => "en".to_string(),
    }
}
