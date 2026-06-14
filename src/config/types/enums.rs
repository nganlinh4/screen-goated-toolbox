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

impl ThemeMode {
    /// Resolve to a concrete dark/light boolean, querying the OS for `System`.
    /// Canonical theme resolver — replaces the inline match scattered across
    /// every overlay/window. For hot per-frame paths that cache the OS lookup,
    /// prefer the cached `system_dark` flags in app/init.rs / app/logic.rs.
    pub fn is_dark(&self) -> bool {
        match self {
            ThemeMode::Dark => true,
            ThemeMode::Light => false,
            ThemeMode::System => crate::gui::utils::is_system_in_dark_mode(),
        }
    }

    /// Web/WebView theme string ("dark"/"light"), resolving `System` against the OS.
    pub fn as_web_str(&self) -> &'static str {
        if self.is_dark() { "dark" } else { "light" }
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
