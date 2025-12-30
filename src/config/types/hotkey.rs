//! Hotkey configuration type.

use serde::{Deserialize, Serialize};

/// Represents a keyboard hotkey binding
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Hotkey {
    /// Virtual key code
    pub code: u32,
    /// Human-readable key name (e.g., "` / ~", "F1")
    pub name: String,
    /// Modifier flags (Ctrl, Alt, Shift, Win)
    pub modifiers: u32,
}

impl Hotkey {
    pub fn new(code: u32, name: &str, modifiers: u32) -> Self {
        Self {
            code,
            name: name.to_string(),
            modifiers,
        }
    }
}
