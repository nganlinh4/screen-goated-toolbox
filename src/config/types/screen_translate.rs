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
    #[serde(default = "default_overlay_opacity")]
    pub overlay_opacity: u8,
    #[serde(default)]
    pub hotkeys: Vec<Hotkey>,
    #[serde(default)]
    pub fullscreen_hotkeys: Vec<Hotkey>,
    #[serde(default)]
    pub fixed_region: Option<ScreenTranslateRegion>,
}

/// Monitor-relative edges in ten-thousandths, independent of desktop origin and DPI.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ScreenTranslateRegion {
    pub monitor: String,
    pub edges: [u16; 4],
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

fn default_overlay_opacity() -> u8 {
    100
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
        self.overlay_opacity = self.overlay_opacity.clamp(10, 100);
        self.fixed_region = self.fixed_region.filter(|region| {
            let [left, top, right, bottom] = region.edges;
            !region.monitor.is_empty()
                && left < right
                && top < bottom
                && right <= 10_000
                && bottom <= 10_000
        });
        self
    }

    pub fn restore_defaults_preserving_hotkeys(&mut self) {
        let hotkeys = std::mem::take(&mut self.hotkeys);
        let fullscreen_hotkeys = std::mem::take(&mut self.fullscreen_hotkeys);
        *self = Self {
            hotkeys,
            fullscreen_hotkeys,
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
            overlay_opacity: default_overlay_opacity(),
            hotkeys: Vec::new(),
            fullscreen_hotkeys: Vec::new(),
            fixed_region: None,
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
            overlay_opacity: 0,
            hotkeys: Vec::new(),
            ..ScreenTranslateSettings::default()
        }
        .normalized();

        assert_eq!(settings.target_language, "Vietnamese");
        assert_eq!(
            settings.translation_model,
            crate::model_config::DEFAULT_TEXT_MODEL_ID
        );
        assert!(settings.translation_prompt.contains("{target_language}"));
        assert_eq!(settings.overlay_opacity, 10);
    }

    #[test]
    fn restoring_defaults_keeps_user_hotkeys() {
        let hotkeys = vec![crate::config::Hotkey::new(9, 10)];
        let mut settings = ScreenTranslateSettings {
            target_language: "Korean".to_string(),
            translation_model: "custom".to_string(),
            translation_prompt: "Custom".to_string(),
            overlay_opacity: 37,
            hotkeys: hotkeys.clone(),
            fullscreen_hotkeys: hotkeys.clone(),
            fixed_region: Some(super::ScreenTranslateRegion {
                monitor: "display".into(),
                edges: [0, 500, 10000, 9500],
            }),
        };

        settings.restore_defaults_preserving_hotkeys();

        assert_eq!(settings.hotkeys, hotkeys);
        assert_eq!(settings.fullscreen_hotkeys, hotkeys);
        assert!(settings.fixed_region.is_none());
        assert_eq!(settings.target_language, "Vietnamese");
        assert_eq!(
            settings.translation_model,
            crate::model_config::DEFAULT_TEXT_MODEL_ID
        );
        assert_eq!(
            settings.translation_prompt,
            ScreenTranslateSettings::default_prompt()
        );
        assert_eq!(settings.overlay_opacity, 100);
    }

    #[test]
    fn legacy_settings_default_to_full_screen_without_new_shortcuts() {
        let settings: ScreenTranslateSettings =
            serde_json::from_str(r#"{"hotkeys":[{"code":121,"modifiers":0}]}"#).unwrap();
        assert_eq!(settings.hotkeys.len(), 1);
        assert!(settings.fullscreen_hotkeys.is_empty());
        assert!(settings.fixed_region.is_none());
        assert_eq!(
            serde_json::from_value::<ScreenTranslateSettings>(
                serde_json::to_value(&settings).unwrap()
            )
            .unwrap(),
            settings
        );
    }

    #[test]
    fn invalid_saved_geometry_is_discarded() {
        let settings = ScreenTranslateSettings {
            fixed_region: Some(super::ScreenTranslateRegion {
                monitor: "display".into(),
                edges: [100, 0, 50, 10000],
            }),
            ..Default::default()
        }
        .normalized();
        assert!(settings.fixed_region.is_none());
    }

    #[test]
    fn fullscreen_shortcuts_participate_in_global_conflict_detection() {
        let mut config = crate::config::Config::default();
        config
            .screen_translate
            .fullscreen_hotkeys
            .push(crate::config::Hotkey::new(0x7b, 3));
        assert!(config.check_hotkey_conflict(0x7b, 3, None).is_some());
    }
}
