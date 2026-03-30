use serde::{Deserialize, Serialize};

use super::Hotkey;

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct BilingualRelayProfile {
    #[serde(default)]
    pub language: String,
    #[serde(default)]
    pub accent: String,
    #[serde(default)]
    pub tone: String,
}

impl BilingualRelayProfile {
    pub fn normalized(&self) -> Self {
        Self {
            language: self.language.trim().to_string(),
            accent: self.accent.trim().to_string(),
            tone: self.tone.trim().to_string(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct BilingualRelaySettings {
    #[serde(default = "default_first_profile")]
    pub first: BilingualRelayProfile,
    #[serde(default = "default_second_profile")]
    pub second: BilingualRelayProfile,
    /// Legacy single hotkey — migrated into `hotkeys` on load.
    #[serde(default, skip_serializing)]
    pub hotkey: Option<Hotkey>,
    #[serde(default)]
    pub hotkeys: Vec<Hotkey>,
    #[serde(default)]
    pub guide_seen: bool,
}

fn default_first_profile() -> BilingualRelayProfile {
    BilingualRelayProfile {
        language: "English".to_string(),
        accent: String::new(),
        tone: String::new(),
    }
}

fn default_second_profile() -> BilingualRelayProfile {
    BilingualRelayProfile {
        language: "Korean".to_string(),
        accent: "Busan".to_string(),
        tone: "polite".to_string(),
    }
}

impl BilingualRelaySettings {
    pub fn normalized(&self) -> Self {
        let mut hotkeys = self.hotkeys.clone();
        // Migrate legacy single hotkey into vec
        if let Some(ref legacy) = self.hotkey {
            if !hotkeys.iter().any(|h| h.code == legacy.code && h.modifiers == legacy.modifiers) {
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
        fn describe(profile: &BilingualRelayProfile) -> String {
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

#[cfg(test)]
mod tests {
    use super::{BilingualRelayProfile, BilingualRelaySettings};

    #[test]
    fn build_system_instruction_omits_blank_optional_fields() {
        let settings = BilingualRelaySettings {
            first: BilingualRelayProfile {
                language: "Korean".to_string(),
                accent: "Busan".to_string(),
                tone: "polite".to_string(),
            },
            second: BilingualRelayProfile {
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
}
