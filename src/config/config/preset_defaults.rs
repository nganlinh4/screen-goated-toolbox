use super::Config;
use crate::config::preset::{Preset, get_default_presets};
use crate::config::types::{PendingPresetModelUpdate, PresetModelDefaults};
use anyhow::{Context, Result};
use semver::Version;

impl Config {
    /// Record the preset model defaults compiled into the old executable. The
    /// new executable compares them before deciding whether a prompt is needed.
    pub fn mark_staged_preset_model_update(&mut self, target_version: String) -> Result<()> {
        Version::parse(&target_version)
            .with_context(|| format!("invalid staged app version {target_version:?}"))?;
        let previous_models = builtin_preset_model_defaults();
        self.pending_preset_model_update = Some(PendingPresetModelUpdate {
            target_version,
            previous_models,
        });
        Ok(())
    }

    /// Return whether the new executable should offer changed preset models.
    /// An update with identical model defaults is consumed silently.
    pub fn prepare_preset_model_update_prompt(&mut self, current_version: &str) -> Result<bool> {
        let Some(pending) = self.pending_preset_model_update.as_ref() else {
            return Ok(false);
        };

        let current = Version::parse(current_version)
            .with_context(|| format!("invalid current app version {current_version:?}"))?;
        let target = Version::parse(&pending.target_version)
            .with_context(|| format!("invalid staged app version {:?}", pending.target_version))?;
        if current < target {
            return Ok(false);
        }

        let current_models = builtin_preset_model_defaults();
        if changed_model_slots(&pending.previous_models, &current_models).is_empty() {
            self.pending_preset_model_update = None;
            return Ok(false);
        }

        Ok(true)
    }

    /// Replace every preset and profile with the supplied defaults. Hotkeys
    /// and favorite stars from the active preset set follow matching preset
    /// IDs into the reset configuration.
    pub fn restore_presets_and_profiles_preserving_user_state(&mut self, defaults: &Self) {
        let previous_presets = self.presets.clone();

        self.presets = defaults.presets.clone();
        restore_preset_user_state(&mut self.presets, &previous_presets);
        self.active_preset_idx = defaults.active_preset_idx;

        self.preset_profiles = defaults.preset_profiles.clone();
        for profile in &mut self.preset_profiles {
            restore_preset_user_state(&mut profile.presets, &previous_presets);
        }
        self.active_preset_profile_idx = defaults.active_preset_profile_idx;
    }

    /// Resolve the one-time post-update choice. Applying performs a three-way
    /// model-only migration: a block changes only when it still uses the old
    /// compiled default, so user-selected models and every other setting stay.
    pub fn finish_preset_model_update(&mut self, apply_models: bool) -> usize {
        let previous_models = self
            .pending_preset_model_update
            .as_ref()
            .map(|pending| pending.previous_models.clone())
            .unwrap_or_default();
        let updated = if apply_models {
            self.apply_changed_builtin_preset_models(&previous_models)
        } else {
            0
        };
        self.pending_preset_model_update = None;
        updated
    }

    fn apply_changed_builtin_preset_models(
        &mut self,
        previous_models: &PresetModelDefaults,
    ) -> usize {
        self.sync_active_profile_from_presets();
        let current_models = builtin_preset_model_defaults();
        let changes = changed_model_slots(previous_models, &current_models);
        let mut updated = 0;

        for profile in &mut self.preset_profiles {
            for preset in &mut profile.presets {
                let Some(preset_changes) = changes.get(&preset.id) else {
                    continue;
                };
                for change in preset_changes {
                    let Some(block) = preset.blocks.get_mut(change.block_index) else {
                        continue;
                    };
                    if block.block_type == change.block_type && block.model == change.previous_model
                    {
                        block.model = change.current_model.clone();
                        updated += 1;
                    }
                }
            }
        }

        self.ensure_preset_profiles();
        updated
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ModelChange {
    block_index: usize,
    block_type: String,
    previous_model: String,
    current_model: String,
}

fn changed_model_slots(
    previous: &PresetModelDefaults,
    current: &PresetModelDefaults,
) -> std::collections::BTreeMap<String, Vec<ModelChange>> {
    let mut changes = std::collections::BTreeMap::new();

    for (preset_id, previous_blocks) in previous {
        let Some(current_blocks) = current.get(preset_id) else {
            continue;
        };
        let preset_changes = previous_blocks
            .iter()
            .zip(current_blocks)
            .enumerate()
            .filter_map(
                |(
                    block_index,
                    ((previous_type, previous_model), (current_type, current_model)),
                )| {
                    if previous_type != current_type || previous_model == current_model {
                        return None;
                    }
                    Some(ModelChange {
                        block_index,
                        block_type: previous_type.clone(),
                        previous_model: previous_model.clone(),
                        current_model: current_model.clone(),
                    })
                },
            )
            .collect::<Vec<_>>();
        if !preset_changes.is_empty() {
            changes.insert(preset_id.clone(), preset_changes);
        }
    }

    changes
}

fn restore_preset_user_state(presets: &mut [Preset], previous_presets: &[Preset]) {
    for preset in presets {
        let Some(previous) = previous_presets
            .iter()
            .find(|previous| previous.id == preset.id)
        else {
            continue;
        };
        preset.hotkeys = previous.hotkeys.clone();
        preset.is_favorite = previous.is_favorite;
    }
}

fn builtin_preset_model_defaults() -> PresetModelDefaults {
    preset_model_defaults(&get_default_presets())
}

fn preset_model_defaults(presets: &[Preset]) -> PresetModelDefaults {
    presets
        .iter()
        .filter(|preset| preset.is_builtin())
        .map(|preset| {
            (
                preset.id.clone(),
                preset
                    .blocks
                    .iter()
                    .map(|block| (block.block_type.clone(), block.model.clone()))
                    .collect(),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{builtin_preset_model_defaults, changed_model_slots, preset_model_defaults};
    use crate::config::preset::get_default_presets;
    use crate::config::types::{PendingPresetModelUpdate, PresetProfile};
    use crate::config::{Config, Hotkey, Preset};

    fn hotkey(code: u32, name: &str) -> Hotkey {
        Hotkey::new(code, name, crate::hotkey::MOD_CONTROL)
    }

    #[test]
    fn comparison_only_detects_existing_default_model_changes() {
        let mut defaults = get_default_presets();
        let baseline_models = builtin_preset_model_defaults();
        assert!(changed_model_slots(&baseline_models, &builtin_preset_model_defaults()).is_empty());

        defaults[0].name.push_str(" user name");
        defaults[0].hotkeys.push(hotkey(0x41, "Ctrl + A"));
        defaults[0].blocks[0].prompt.push_str(" user prompt");
        assert!(
            changed_model_slots(&baseline_models, &preset_model_defaults(&defaults)).is_empty()
        );

        defaults[0].blocks[0].model.push_str("-changed");
        assert_eq!(
            changed_model_slots(&baseline_models, &preset_model_defaults(&defaults)).len(),
            1
        );

        let mut with_new_preset = defaults;
        let added = Preset {
            id: "preset_new-after-update".to_string(),
            ..Default::default()
        };
        with_new_preset.push(added);
        assert_eq!(
            changed_model_slots(&baseline_models, &preset_model_defaults(&with_new_preset)).len(),
            1,
            "a new preset must not create another migration"
        );
    }

    #[test]
    fn prompt_waits_for_target_and_consumes_identical_models() {
        let models = builtin_preset_model_defaults();
        let mut config = Config {
            pending_preset_model_update: Some(PendingPresetModelUpdate {
                target_version: "2.0.0".to_string(),
                previous_models: models,
            }),
            ..Default::default()
        };

        assert!(!config.prepare_preset_model_update_prompt("1.9.9").unwrap());
        assert!(config.pending_preset_model_update.is_some());
        assert!(!config.prepare_preset_model_update_prompt("2.0.0").unwrap());
        assert!(config.pending_preset_model_update.is_none());
    }

    #[test]
    fn prompt_appears_for_changed_models_at_or_after_target() {
        for current in ["2.0.0", "2.1.0"] {
            let mut previous_models = builtin_preset_model_defaults();
            previous_models
                .values_mut()
                .find_map(|blocks| blocks.first_mut())
                .unwrap()
                .1
                .push_str("-old");
            let mut config = Config {
                pending_preset_model_update: Some(PendingPresetModelUpdate {
                    target_version: "2.0.0".to_string(),
                    previous_models,
                }),
                ..Default::default()
            };

            assert!(config.prepare_preset_model_update_prompt(current).unwrap());
            assert!(config.pending_preset_model_update.is_some());
        }
    }

    #[test]
    fn manual_restore_replaces_profiles_and_custom_presets_but_keeps_active_user_state() {
        let defaults = Config::default();
        let mut edited_builtin = defaults.presets[0].clone();
        edited_builtin.name = "User-edited built-in".to_string();
        edited_builtin.hotkeys = vec![hotkey(0x41, "Ctrl + A")];
        edited_builtin.is_favorite = !defaults.presets[0].is_favorite;
        let custom = Preset {
            id: "custom-workflow".to_string(),
            name: "Custom workflow".to_string(),
            hotkeys: vec![hotkey(0x42, "Ctrl + B")],
            ..Default::default()
        };
        let profile = PresetProfile {
            id: "profile-user".to_string(),
            name: "User profile".to_string(),
            presets: vec![edited_builtin.clone(), custom.clone()],
            active_preset_idx: 1,
        };
        let mut config = Config {
            presets: profile.presets.clone(),
            active_preset_idx: 1,
            preset_profiles: vec![profile],
            screen_record_hotkeys: vec![hotkey(0x43, "Ctrl + C")],
            computer_control_hotkeys: vec![hotkey(0x44, "Ctrl + D")],
            ..Default::default()
        };
        config.translation_gummy.hotkey = Some(hotkey(0x45, "Ctrl + E"));
        config.translation_gummy.hotkeys = vec![hotkey(0x46, "Ctrl + F")];
        let global_hotkeys = (
            config.screen_record_hotkeys.clone(),
            config.computer_control_hotkeys.clone(),
            config.translation_gummy.hotkey.clone(),
            config.translation_gummy.hotkeys.clone(),
        );

        config.restore_presets_and_profiles_preserving_user_state(&defaults);

        assert_eq!(
            config.active_preset_profile_idx,
            defaults.active_preset_profile_idx
        );
        assert_eq!(config.active_preset_idx, defaults.active_preset_idx);
        assert_eq!(config.preset_profiles.len(), defaults.preset_profiles.len());
        assert_eq!(config.preset_profiles[0].id, defaults.preset_profiles[0].id);
        assert_eq!(
            config.preset_profiles[0].name,
            defaults.preset_profiles[0].name
        );
        assert!(config.presets.iter().all(|preset| preset.id != custom.id));
        let restored = config
            .presets
            .iter()
            .find(|preset| preset.id == edited_builtin.id)
            .unwrap();
        assert_eq!(restored.name, defaults.presets[0].name);
        assert_eq!(restored.hotkeys, edited_builtin.hotkeys);
        assert_eq!(restored.is_favorite, edited_builtin.is_favorite);
        let restored_in_profile = config.preset_profiles[0]
            .presets
            .iter()
            .find(|preset| preset.id == edited_builtin.id)
            .unwrap();
        assert_eq!(restored_in_profile.hotkeys, edited_builtin.hotkeys);
        assert_eq!(restored_in_profile.is_favorite, edited_builtin.is_favorite);
        for restored_default in config
            .presets
            .iter()
            .filter(|preset| preset.id != edited_builtin.id)
        {
            let expected = defaults
                .presets
                .iter()
                .find(|preset| preset.id == restored_default.id)
                .unwrap();
            assert_eq!(restored_default.hotkeys, expected.hotkeys);
            assert_eq!(restored_default.is_favorite, expected.is_favorite);
        }
        assert_eq!(
            global_hotkeys,
            (
                config.screen_record_hotkeys,
                config.computer_control_hotkeys,
                config.translation_gummy.hotkey,
                config.translation_gummy.hotkeys,
            )
        );
    }

    #[test]
    fn individual_preset_restore_keeps_hotkeys_and_favorite_star() {
        let defaults = get_default_presets();
        let mut preset = defaults[0].clone();
        preset.hotkeys = vec![hotkey(0x47, "Ctrl + G")];
        preset.is_favorite = !defaults[0].is_favorite;
        preset.blocks.clear();
        let expected_favorite = preset.is_favorite;

        preset.replace_config_preserving_user_state(&defaults[0]);

        assert_eq!(
            serde_json::to_value(&preset.blocks).unwrap(),
            serde_json::to_value(&defaults[0].blocks).unwrap()
        );
        assert_eq!(preset.hotkeys, vec![hotkey(0x47, "Ctrl + G")]);
        assert_eq!(preset.is_favorite, expected_favorite);
    }

    #[test]
    fn update_changes_only_unchanged_old_models_across_profiles() {
        let defaults = get_default_presets();
        let current_model = defaults[0].blocks[0].model.clone();
        let old_model = format!("{current_model}-previous");
        let mut previous_models = builtin_preset_model_defaults();
        previous_models.get_mut(&defaults[0].id).unwrap()[0].1 = old_model.clone();

        let mut inherited = defaults[0].clone();
        inherited.name = "User preset name".to_string();
        inherited.blocks[0].model = old_model.clone();
        inherited.blocks[0].prompt = "User prompt".to_string();
        inherited.hotkeys = vec![hotkey(0x48, "Ctrl + H")];
        inherited.is_favorite = true;

        let mut overridden = inherited.clone();
        overridden.blocks[0].model = "user-selected-model".to_string();
        overridden.hotkeys = vec![hotkey(0x49, "Ctrl + I")];

        let marker = PendingPresetModelUpdate {
            target_version: "2.0.0".to_string(),
            previous_models,
        };
        let mut base = Config {
            presets: vec![inherited.clone()],
            preset_profiles: vec![
                PresetProfile {
                    id: "profile-inherited".to_string(),
                    name: "Inherited".to_string(),
                    presets: vec![inherited],
                    active_preset_idx: 0,
                },
                PresetProfile {
                    id: "profile-overridden".to_string(),
                    name: "Overridden".to_string(),
                    presets: vec![overridden],
                    active_preset_idx: 0,
                },
            ],
            pending_preset_model_update: Some(marker),
            screen_record_hotkeys: vec![hotkey(0x50, "Ctrl + P")],
            computer_control_hotkeys: vec![hotkey(0x51, "Ctrl + Q")],
            ..Default::default()
        };
        base.translation_gummy.hotkey = Some(hotkey(0x52, "Ctrl + R"));
        base.translation_gummy.hotkeys = vec![hotkey(0x53, "Ctrl + S")];

        let mut expected_skipped = base.clone();
        expected_skipped.pending_preset_model_update = None;
        let mut skipped = base.clone();
        assert_eq!(skipped.finish_preset_model_update(false), 0);
        assert_eq!(
            serde_json::to_value(&skipped).unwrap(),
            serde_json::to_value(&expected_skipped).unwrap(),
            "declining must only consume the one-time marker"
        );

        let mut expected_applied = base.clone();
        expected_applied.pending_preset_model_update = None;
        expected_applied.presets[0].blocks[0].model = current_model.clone();
        expected_applied.preset_profiles[0].presets[0].blocks[0].model = current_model.clone();
        let mut applied = base;
        assert_eq!(applied.finish_preset_model_update(true), 1);
        assert_eq!(
            serde_json::to_value(&applied).unwrap(),
            serde_json::to_value(&expected_applied).unwrap(),
            "applying must change only inherited model slots and consume the marker"
        );
    }
}
