use serde::{Deserialize, Serialize};

fn default_adaptive_enabled() -> bool {
    true
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct LiveModelOverrides {
    #[serde(default)]
    pub pinned: Vec<String>,
    #[serde(default)]
    pub excluded: Vec<String>,
}

impl LiveModelOverrides {
    pub fn pin(&mut self, id: &str) {
        self.excluded.retain(|excluded| excluded != id);
        if !self.pinned.iter().any(|pinned| pinned == id) {
            self.pinned.push(id.to_string());
        }
    }

    pub fn exclude(&mut self, id: &str) {
        self.pinned.retain(|pinned| pinned != id);
        if !self.excluded.iter().any(|excluded| excluded == id) {
            self.excluded.push(id.to_string());
        }
    }

    pub fn normalize(&mut self) {
        deduplicate(&mut self.pinned);
        deduplicate(&mut self.excluded);
        self.pinned
            .retain(|id| !self.excluded.iter().any(|excluded| excluded == id));
    }
}

fn deduplicate(values: &mut Vec<String>) {
    let mut seen = std::collections::HashSet::new();
    values.retain(|value| seen.insert(value.clone()));
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AdaptiveModelPriority {
    #[serde(default = "default_adaptive_enabled")]
    pub image_to_text: bool,
    #[serde(default = "default_adaptive_enabled")]
    pub text_to_text: bool,
    #[serde(default)]
    pub image_to_text_overrides: LiveModelOverrides,
    #[serde(default)]
    pub text_to_text_overrides: LiveModelOverrides,
}

impl Default for AdaptiveModelPriority {
    fn default() -> Self {
        Self {
            image_to_text: true,
            text_to_text: true,
            image_to_text_overrides: LiveModelOverrides::default(),
            text_to_text_overrides: LiveModelOverrides::default(),
        }
    }
}

fn default_image_to_text_priority_chain() -> Vec<String> {
    crate::model_config::default_image_to_text_priority_chain_ids()
        .iter()
        .map(|id| (*id).to_string())
        .collect()
}

fn default_text_to_text_priority_chain() -> Vec<String> {
    crate::model_config::default_text_to_text_priority_chain_ids()
        .iter()
        .map(|id| (*id).to_string())
        .collect()
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ModelPriorityChains {
    #[serde(default = "default_image_to_text_priority_chain")]
    pub image_to_text: Vec<String>,
    #[serde(default = "default_text_to_text_priority_chain")]
    pub text_to_text: Vec<String>,
}

impl Default for ModelPriorityChains {
    fn default() -> Self {
        Self {
            image_to_text: default_image_to_text_priority_chain(),
            text_to_text: default_text_to_text_priority_chain(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_adaptive_settings_default_both_chains_on() {
        let settings: AdaptiveModelPriority = serde_json::from_str("{}").unwrap();
        assert!(settings.image_to_text);
        assert!(settings.text_to_text);
        assert_eq!(
            settings.image_to_text_overrides,
            LiveModelOverrides::default()
        );
        assert_eq!(
            settings.text_to_text_overrides,
            LiveModelOverrides::default()
        );
    }

    #[test]
    fn latest_row_override_wins_and_saved_duplicates_are_normalized() {
        let mut overrides = LiveModelOverrides {
            pinned: vec!["a".into(), "a".into(), "b".into()],
            excluded: vec!["b".into(), "b".into()],
        };
        overrides.normalize();
        assert_eq!(overrides.pinned, ["a"]);
        assert_eq!(overrides.excluded, ["b"]);

        overrides.pin("b");
        assert_eq!(overrides.pinned, ["a", "b"]);
        assert!(overrides.excluded.is_empty());
        overrides.exclude("a");
        assert_eq!(overrides.pinned, ["b"]);
        assert_eq!(overrides.excluded, ["a"]);
    }
}
