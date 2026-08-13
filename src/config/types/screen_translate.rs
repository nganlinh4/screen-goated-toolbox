//! Settings for the in-place Screen Translate mini app.

use serde::{Deserialize, Serialize};

use super::Hotkey;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ScreenTranslateSettings {
    #[serde(default = "default_target_language")]
    pub target_language: String,
    #[serde(default)]
    pub hotkeys: Vec<Hotkey>,
    #[serde(default)]
    pub dismiss_button_impressions: u8,
}

fn default_target_language() -> String {
    "Vietnamese".to_string()
}

impl ScreenTranslateSettings {
    pub fn normalized(mut self) -> Self {
        self.target_language = self.target_language.trim().to_string();
        if self.target_language.is_empty() {
            self.target_language = default_target_language();
        }
        self
    }
}

impl Default for ScreenTranslateSettings {
    fn default() -> Self {
        Self {
            target_language: default_target_language(),
            hotkeys: Vec::new(),
            dismiss_button_impressions: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ScreenTranslateSettings;

    #[test]
    fn normalization_restores_a_blank_target_language() {
        let settings = ScreenTranslateSettings {
            target_language: "  ".to_string(),
            hotkeys: Vec::new(),
            dismiss_button_impressions: 0,
        }
        .normalized();

        assert_eq!(settings.target_language, "Vietnamese");
    }
}
