use serde::{Deserialize, Serialize};

use crate::config::preset::{Preset, get_default_presets};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PresetProfile {
    #[serde(default = "generate_profile_id")]
    pub id: String,

    #[serde(default = "default_profile_name")]
    pub name: String,

    #[serde(default = "get_default_presets")]
    pub presets: Vec<Preset>,

    #[serde(default)]
    pub active_preset_idx: usize,
}

impl PresetProfile {
    pub fn new_default(presets: Vec<Preset>, active_preset_idx: usize) -> Self {
        Self {
            id: generate_profile_id(),
            name: default_profile_name(),
            presets,
            active_preset_idx,
        }
    }

    pub fn cloned_from(source: &PresetProfile, name: String) -> Self {
        Self {
            id: generate_profile_id(),
            name,
            presets: source.presets.clone(),
            active_preset_idx: source
                .active_preset_idx
                .min(source.presets.len().saturating_sub(1)),
        }
    }
}

fn generate_profile_id() -> String {
    format!(
        "profile_{:x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    )
}

fn default_profile_name() -> String {
    "Default".to_string()
}
