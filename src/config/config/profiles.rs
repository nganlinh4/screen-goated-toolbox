use super::Config;
use crate::config::types::PresetProfile;

impl Config {
    pub fn ensure_preset_profiles(&mut self) {
        if self.preset_profiles.is_empty() {
            self.preset_profiles = vec![PresetProfile::new_default(
                self.presets.clone(),
                self.active_preset_idx,
            )];
            self.active_preset_profile_idx = 0;
            return;
        }

        self.active_preset_profile_idx = self
            .active_preset_profile_idx
            .min(self.preset_profiles.len().saturating_sub(1));

        if let Some(profile) = self.preset_profiles.get(self.active_preset_profile_idx) {
            self.presets = profile.presets.clone();
            self.active_preset_idx = profile
                .active_preset_idx
                .min(self.presets.len().saturating_sub(1));
        }
    }

    pub fn sync_active_profile_from_presets(&mut self) {
        if self.preset_profiles.is_empty() {
            self.preset_profiles = vec![PresetProfile::new_default(
                self.presets.clone(),
                self.active_preset_idx,
            )];
            self.active_preset_profile_idx = 0;
            return;
        }

        self.active_preset_profile_idx = self
            .active_preset_profile_idx
            .min(self.preset_profiles.len().saturating_sub(1));
        if let Some(profile) = self.preset_profiles.get_mut(self.active_preset_profile_idx) {
            profile.presets = self.presets.clone();
            profile.active_preset_idx = self
                .active_preset_idx
                .min(self.presets.len().saturating_sub(1));
        }
    }

    pub fn switch_preset_profile(&mut self, idx: usize) {
        if self.preset_profiles.is_empty() {
            self.sync_active_profile_from_presets();
        }
        if idx >= self.preset_profiles.len() || idx == self.active_preset_profile_idx {
            return;
        }

        self.sync_active_profile_from_presets();
        self.active_preset_profile_idx = idx;
        if let Some(profile) = self.preset_profiles.get(idx) {
            self.presets = profile.presets.clone();
            self.active_preset_idx = profile
                .active_preset_idx
                .min(self.presets.len().saturating_sub(1));
        }
    }

    pub fn add_preset_profile_from_active(&mut self) {
        self.sync_active_profile_from_presets();
        let source_idx = self.active_preset_profile_idx;
        let base = self.preset_profiles[source_idx].name.clone();
        let name = self.next_profile_copy_name(&base);
        let new_profile = PresetProfile::cloned_from(&self.preset_profiles[source_idx], name);
        self.preset_profiles.insert(source_idx + 1, new_profile);
        self.switch_preset_profile(source_idx + 1);
    }

    pub fn delete_preset_profile(&mut self, idx: usize) {
        if self.preset_profiles.len() <= 1 || idx >= self.preset_profiles.len() {
            return;
        }

        let old_active_idx = self.active_preset_profile_idx;
        self.sync_active_profile_from_presets();
        self.preset_profiles.remove(idx);
        self.active_preset_profile_idx = if idx == old_active_idx {
            idx.saturating_sub(1)
        } else if idx < old_active_idx {
            old_active_idx.saturating_sub(1)
        } else {
            old_active_idx
        }
        .min(self.preset_profiles.len().saturating_sub(1));
        if let Some(profile) = self.preset_profiles.get(self.active_preset_profile_idx) {
            self.presets = profile.presets.clone();
            self.active_preset_idx = profile
                .active_preset_idx
                .min(self.presets.len().saturating_sub(1));
        }
    }

    fn next_profile_copy_name(&self, base: &str) -> String {
        let mut candidate = format!("{base} Copy");
        let mut counter = 1;
        while self
            .preset_profiles
            .iter()
            .any(|profile| profile.name == candidate)
        {
            candidate = format!("{base} Copy {counter}");
            counter += 1;
        }
        candidate
    }

    /// Checks if a hotkey combination conflicts with any existing hotkeys.
    /// Returns the name of the conflicting item if found.
    pub fn check_hotkey_conflict(
        &self,
        vk: u32,
        mods: u32,
        exclude_preset_idx: Option<usize>,
    ) -> Option<String> {
        for h in &self.screen_record_hotkeys {
            if h.code == vk && h.modifiers == mods {
                return Some(format!(
                    "Conflict with global hotkey '{}' (Screen Record)",
                    h.name
                ));
            }
        }

        for h in &self.translation_gummy.hotkeys {
            if h.code == vk && h.modifiers == mods {
                return Some(format!(
                    "Conflict with global hotkey '{}' (Translation Gummy)",
                    h.name
                ));
            }
        }

        for h in &self.computer_control_hotkeys {
            if h.code == vk && h.modifiers == mods {
                return Some(format!(
                    "Conflict with global hotkey '{}' (Computer Control)",
                    h.name
                ));
            }
        }

        for (idx, preset) in self.presets.iter().enumerate() {
            if Some(idx) == exclude_preset_idx {
                continue;
            }
            for h in &preset.hotkeys {
                if h.code == vk && h.modifiers == mods {
                    return Some(format!(
                        "Conflict with '{}' in preset '{}'",
                        h.name, preset.name
                    ));
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::Config;
    use crate::config::{Hotkey, Preset};

    #[test]
    fn computer_control_hotkeys_participate_in_global_conflict_checks() {
        let global_key = Hotkey::new(0x75, "F6", 0);
        let preset_key = Hotkey::new(0x41, "Ctrl + A", crate::hotkey::MOD_CONTROL);
        let config = Config {
            computer_control_hotkeys: vec![global_key.clone()],
            presets: vec![Preset {
                hotkeys: vec![preset_key.clone()],
                ..Default::default()
            }],
            ..Default::default()
        };

        assert!(
            config
                .check_hotkey_conflict(global_key.code, global_key.modifiers, None)
                .is_some()
        );
        assert!(
            config
                .check_hotkey_conflict(global_key.code, global_key.modifiers, Some(0))
                .is_some()
        );
        assert!(
            config
                .check_hotkey_conflict(preset_key.code, preset_key.modifiers, None)
                .is_some()
        );
    }
}
