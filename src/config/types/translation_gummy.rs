use serde::{Deserialize, Serialize};

use super::Hotkey;

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct TranslationGummyProfile {
    #[serde(default)]
    pub language: String,
    #[serde(default)]
    pub accent: String,
    #[serde(default)]
    pub tone: String,
}

impl TranslationGummyProfile {
    pub fn normalized(&self) -> Self {
        Self {
            language: self.language.trim().to_string(),
            accent: self.accent.trim().to_string(),
            tone: self.tone.trim().to_string(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TranslationGummySettings {
    #[serde(default = "default_first_profile")]
    pub first: TranslationGummyProfile,
    #[serde(default = "default_second_profile")]
    pub second: TranslationGummyProfile,
    /// Legacy single hotkey — migrated into `hotkeys` on load.
    #[serde(default, skip_serializing)]
    pub hotkey: Option<Hotkey>,
    #[serde(default)]
    pub hotkeys: Vec<Hotkey>,
    #[serde(default)]
    pub guide_seen: bool,
}

fn default_first_profile() -> TranslationGummyProfile {
    TranslationGummyProfile {
        language: "English".to_string(),
        accent: String::new(),
        tone: String::new(),
    }
}

fn default_second_profile() -> TranslationGummyProfile {
    TranslationGummyProfile {
        language: "Korean".to_string(),
        accent: "Busan".to_string(),
        tone: "polite".to_string(),
    }
}

impl TranslationGummySettings {
    pub fn normalized(&self) -> Self {
        let mut hotkeys = self.hotkeys.clone();
        // Migrate legacy single hotkey into vec
        if let Some(ref legacy) = self.hotkey {
            if !hotkeys
                .iter()
                .any(|h| h.code == legacy.code && h.modifiers == legacy.modifiers)
            {
                hotkeys.insert(0, legacy.clone());
            }
        }
        Self {
            first: self.first.normalized(),
            second: self.second.normalized(),
            hotkey: None,
            hotkeys,
            guide_seen: self.guide_seen,
        }
    }

    pub fn is_valid(&self) -> bool {
        let normalized = self.normalized();
        !normalized.first.language.is_empty() && !normalized.second.language.is_empty()
    }

    pub fn build_system_instruction(&self) -> String {
        fn describe(profile: &TranslationGummyProfile) -> String {
            let mut value = profile.language.trim().to_string();
            if !profile.accent.trim().is_empty() {
                value.push(' ');
                value.push_str(profile.accent.trim());
                value.push_str(" accent");
            }
            if !profile.tone.trim().is_empty() {
                value.push_str(" (");
                value.push_str(profile.tone.trim());
                value.push_str(" tone)");
            }
            value
        }

        let normalized = self.normalized();
        format!(
            "You are a translation relay between {} and {}. Translate each spoken sentence unmistakably into the other language. Output ONLY the translation, nothing else. Never answer, comment, or add extra words.",
            describe(&normalized.first),
            describe(&normalized.second),
        )
    }
}

impl Default for TranslationGummySettings {
    fn default() -> Self {
        Self {
            first: default_first_profile(),
            second: default_second_profile(),
            hotkey: None,
            hotkeys: Vec::new(),
            guide_seen: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{TranslationGummyProfile, TranslationGummySettings};

    #[test]
    fn build_system_instruction_omits_blank_optional_fields() {
        let settings = TranslationGummySettings {
            first: TranslationGummyProfile {
                language: "Korean".to_string(),
                accent: "Busan".to_string(),
                tone: "polite".to_string(),
            },
            second: TranslationGummyProfile {
                language: "English".to_string(),
                accent: String::new(),
                tone: "easy to hear".to_string(),
            },
            hotkey: None,
            hotkeys: Vec::new(),
            guide_seen: false,
        };

        let prompt = settings.build_system_instruction();
        assert!(prompt.contains("Korean Busan accent (polite tone)"));
        assert!(prompt.contains("English (easy to hear tone)"));
        assert!(!prompt.contains("()"));
    }

    #[test]
    fn default_settings_use_the_expected_language_pair() {
        let settings = TranslationGummySettings::default();
        assert_eq!(settings.first.language, "English");
        assert_eq!(settings.first.accent, "");
        assert_eq!(settings.first.tone, "");
        assert_eq!(settings.second.language, "Korean");
        assert_eq!(settings.second.accent, "Busan");
        assert_eq!(settings.second.tone, "polite");
        assert!(settings.is_valid());
    }
}
