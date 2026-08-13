//! Settings for the in-place Screen Translate mini app.

use serde::{Deserialize, Serialize};

use super::Hotkey;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ScreenTranslateSettings {
    #[serde(default = "default_target_language")]
    pub target_language: String,
    #[serde(default = "default_translation_model")]
    pub translation_model: String,
    #[serde(default = "default_translation_prompt")]
    pub translation_prompt: String,
    #[serde(default)]
    pub hotkeys: Vec<Hotkey>,
}

fn default_target_language() -> String {
    "Vietnamese".to_string()
}

fn default_translation_model() -> String {
    crate::model_config::DEFAULT_TEXT_MODEL_ID.to_string()
}

fn default_translation_prompt() -> String {
    "Translate every readable text region into {target_language}. Preserve meaning, tone, names, numbers, and punctuation."
        .to_string()
}

impl ScreenTranslateSettings {
    pub fn default_prompt() -> String {
        default_translation_prompt()
    }

    pub fn normalized(mut self) -> Self {
        self.target_language = self.target_language.trim().to_string();
        if self.target_language.is_empty() {
            self.target_language = default_target_language();
        }
        self.translation_model = self.translation_model.trim().to_string();
        if self.translation_model.is_empty() {
            self.translation_model = default_translation_model();
        }
        self.translation_prompt = self.translation_prompt.trim().to_string();
        if self.translation_prompt.is_empty() {
            self.translation_prompt = default_translation_prompt();
        }
        self
    }

    pub fn restore_defaults_preserving_hotkeys(&mut self) {
        let hotkeys = std::mem::take(&mut self.hotkeys);
        *self = Self {
            hotkeys,
            ..Self::default()
        };
    }
}

impl Default for ScreenTranslateSettings {
    fn default() -> Self {
        Self {
            target_language: default_target_language(),
            translation_model: default_translation_model(),
            translation_prompt: default_translation_prompt(),
            hotkeys: Vec::new(),
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
            translation_model: "  ".to_string(),
            translation_prompt: "  ".to_string(),
            hotkeys: Vec::new(),
        }
        .normalized();

        assert_eq!(settings.target_language, "Vietnamese");
        assert_eq!(
            settings.translation_model,
            crate::model_config::DEFAULT_TEXT_MODEL_ID
        );
        assert!(settings.translation_prompt.contains("{target_language}"));
    }

    #[test]
    fn restoring_defaults_keeps_user_hotkeys() {
        let hotkeys = vec![crate::config::Hotkey::new(9, "Translate", 10)];
        let mut settings = ScreenTranslateSettings {
            target_language: "Korean".to_string(),
            translation_model: "custom".to_string(),
            translation_prompt: "Custom".to_string(),
            hotkeys: hotkeys.clone(),
        };

        settings.restore_defaults_preserving_hotkeys();

        assert_eq!(settings.hotkeys, hotkeys);
        assert_eq!(settings.target_language, "Vietnamese");
        assert_eq!(
            settings.translation_model,
            crate::model_config::DEFAULT_TEXT_MODEL_ID
        );
        assert_eq!(
            settings.translation_prompt,
            ScreenTranslateSettings::default_prompt()
        );
    }
}
