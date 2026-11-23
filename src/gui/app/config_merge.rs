//! Merge edits made in the settings window with changes made by other views.

use anyhow::{Result, bail};
use serde_json::Value;

use crate::config::Config;
use crate::gui::locale::LocaleText;
use crate::gui::theme::AppTheme;
use eframe::egui;

pub(super) struct ConfigConflict {
    pub base: Config,
    pub edited: Config,
}

impl ConfigConflict {
    pub(super) fn capture_frame_edits(&mut self, edited: &Config) {
        self.edited = edited.clone();
    }
}

pub(super) fn merge_config(base: &Config, edited: &Config, current: &Config) -> Result<Config> {
    let base = serde_json::to_value(base)?;
    let edited = serde_json::to_value(edited)?;
    let current = serde_json::to_value(current)?;
    Ok(serde_json::from_value(merge_value(
        &base, &edited, &current, false,
    )?)?)
}

pub(super) fn merge_settings_edits(
    base: &Config,
    edited: &Config,
    current: &Config,
) -> Result<Config> {
    let base = serde_json::to_value(base)?;
    let edited = serde_json::to_value(edited)?;
    let current = serde_json::to_value(current)?;
    Ok(serde_json::from_value(merge_value(
        &base, &edited, &current, true,
    )?)?)
}

fn merge_value(base: &Value, edited: &Value, current: &Value, prefer_edits: bool) -> Result<Value> {
    if edited == base {
        return Ok(current.clone());
    }
    if current == base || current == edited {
        return Ok(edited.clone());
    }
    match (base, edited, current) {
        (Value::Object(base), Value::Object(edited), Value::Object(current)) => {
            let mut merged = current.clone();
            for (key, before) in base {
                if !edited.contains_key(key) {
                    if !prefer_edits && current.get(key).is_some_and(|now| now != before) {
                        bail!("concurrent settings removal conflict");
                    }
                    merged.remove(key);
                } else if let Some(after) = edited.get(key)
                    && before != after
                {
                    let new_value = match current.get(key) {
                        Some(now) => merge_value(before, after, now, prefer_edits)?,
                        None if prefer_edits => after.clone(),
                        None => bail!("concurrent settings removal conflict"),
                    };
                    merged.insert(key.clone(), new_value);
                }
            }
            for (key, value) in edited {
                if !base.contains_key(key) {
                    if !prefer_edits && current.get(key).is_some_and(|now| now != value) {
                        bail!("concurrent settings insertion conflict");
                    }
                    merged.insert(key.clone(), value.clone());
                }
            }
            Ok(Value::Object(merged))
        }
        // Arrays are whole ordered values: merging elements by position could
        // silently combine different entries after a concurrent reordering.
        _ if prefer_edits => Ok(edited.clone()),
        _ => bail!("concurrent settings value conflict"),
    }
}

fn profile_id(config: &Config) -> Option<&str> {
    config
        .preset_profiles
        .get(config.active_preset_profile_idx)
        .map(|p| p.id.as_str())
}

fn remap_preset_index(old: &Config, new: &Config, index: Option<usize>) -> Option<usize> {
    if profile_id(old) != profile_id(new) {
        return None;
    }
    let id = &old.presets.get(index?)?.id;
    if old.presets.iter().filter(|p| &p.id == id).count() != 1 {
        return None;
    }
    let mut matches = new.presets.iter().enumerate().filter(|(_, p)| &p.id == id);
    let (index, _) = matches.next()?;
    matches.next().is_none().then_some(index)
}

impl super::SettingsApp {
    pub(super) fn refresh_shared_settings(&mut self, ctx: &egui::Context) {
        // Refresh before an edit or returning from another window, not on every
        // animation/pointer-move frame. A drag retains its already-open intent.
        let editing = ctx.input(|input| {
            input.events.iter().any(|event| {
                matches!(
                    event,
                    egui::Event::WindowFocused(true)
                        | egui::Event::PointerButton { pressed: true, .. }
                        | egui::Event::Key { pressed: true, .. }
                        | egui::Event::Text(_)
                        | egui::Event::Paste(_)
                        | egui::Event::MouseWheel { .. }
                )
            })
        });
        if !editing {
            return;
        }
        let current = self.app_state_ref.lock().unwrap().config.clone();
        match merge_settings_edits(&self.config_baseline, &self.config, &current) {
            Ok(merged) => {
                if let Err(error) = self
                    .config_save
                    .absorb_shared_changes(&self.config_baseline, &current)
                {
                    crate::log_info!("[config] could not refresh pending save: {error}");
                    return;
                }
                self.refresh_preset_editors(&merged);
                self.config = merged;
                self.config_baseline = current;
            }
            Err(error) => crate::log_info!("[config] could not refresh settings: {error}"),
        }
    }

    fn refresh_preset_editors(&mut self, merged: &Config) {
        let same_profile = profile_id(&self.config) == profile_id(merged);
        if same_profile
            && serde_json::to_value(&self.config.presets).ok()
                == serde_json::to_value(&merged.presets).ok()
        {
            return;
        }
        self.current_preset_idx = remap_preset_index(&self.config, merged, self.current_preset_idx);
        if let crate::gui::settings_ui::ViewMode::Preset(index) = self.view_mode {
            self.view_mode = remap_preset_index(&self.config, merged, Some(index)).map_or(
                crate::gui::settings_ui::ViewMode::Global,
                crate::gui::settings_ui::ViewMode::Preset,
            );
        }
        self.recording_hotkey_for_preset = None;
        self.hotkey_conflict_msg = None;
        self.snarl = None;
        self.last_edited_preset_key = None;
    }

    pub(super) fn render_config_save_failure(&mut self, ui: &mut egui::Ui) {
        if self.config_conflict.is_none() {
            return;
        }
        let text = LocaleText::get(&self.config.ui_language);
        let theme = AppTheme::from_ui(ui);
        let mut apply = None;
        crate::gui::widgets::material_modal(
            ui.ctx(),
            &theme,
            egui::Id::new("settings_save_failure"),
            |ui| {
                ui.set_width(520.0);
                ui.heading(text.shell.settings_save_failed_title);
                ui.add_space(8.0);
                ui.label(text.shell.settings_save_failed_body);
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    if crate::gui::widgets::filled_button(
                        ui,
                        text.shell.settings_save_failed_retry,
                        theme.accent_fill(),
                        theme.on_accent(),
                        16,
                    )
                    .clicked()
                    {
                        apply = Some(true);
                    }
                    if crate::gui::widgets::filled_button(
                        ui,
                        text.shell.settings_save_failed_discard,
                        theme.neutral_fill(),
                        theme.on_surface(),
                        16,
                    )
                    .clicked()
                    {
                        apply = Some(false);
                    }
                });
            },
        );
        let Some(apply) = apply else { return };
        let current = self.app_state_ref.lock().unwrap().config.clone();
        let Some(conflict) = self.config_conflict.as_ref() else {
            return;
        };
        if apply {
            self.config_baseline = conflict.base.clone();
            self.config = conflict.edited.clone();
            self.config_conflict = None;
            self.save_and_sync();
            // Retry writes now, so a disk that is still failing reports again.
            self.flush_config_save_now();
            return;
        }
        let edited = current.clone();
        self.config_baseline = current;
        self.config = edited;
        self.config_conflict = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn settings_latest_action_wins_without_reverting_other_saved_values() {
        let base = Config::default();
        let mut edited = base.clone();
        edited.max_history_items += 1;
        let mut current = base.clone();
        current.max_history_items += 2;
        current.tts_playground.draft_text = "saved in another view".into();
        let merged = merge_settings_edits(&base, &edited, &current).unwrap();
        assert_eq!(merged.max_history_items, edited.max_history_items);
        assert_eq!(
            merged.tts_playground.draft_text,
            current.tts_playground.draft_text
        );
        // An untouched settings window follows the shared value instead.
        let refreshed = merge_settings_edits(&base, &base, &current).unwrap();
        assert_eq!(refreshed.max_history_items, current.max_history_items);
    }

    #[test]
    fn settings_ordered_edits_stay_coherent_and_keep_unrelated_settings() {
        let base = Config::default();
        let mut edited = base.clone();
        edited.realtime_custom_vocabulary = vec!["first".into(), "second".into()];
        let mut current = base.clone();
        current.realtime_custom_vocabulary = vec!["second".into(), "first".into()];
        current.max_history_items += 1;
        let merged = merge_settings_edits(&base, &edited, &current).unwrap();
        assert_eq!(
            merged.realtime_custom_vocabulary,
            edited.realtime_custom_vocabulary
        );
        assert_eq!(merged.max_history_items, current.max_history_items);
    }

    #[test]
    fn settings_refresh_preserves_selection_identity_and_rejects_deleted_or_ambiguous_rows() {
        let old = Config {
            presets: vec![
                crate::config::Preset {
                    id: "one".into(),
                    ..Default::default()
                },
                crate::config::Preset {
                    id: "two".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let mut current = old.clone();
        current.presets.swap(0, 1);
        assert_eq!(remap_preset_index(&old, &current, Some(0)), Some(1));
        current.presets.pop();
        assert_eq!(remap_preset_index(&old, &current, Some(0)), None);
        current.presets = vec![old.presets[0].clone(), old.presets[0].clone()];
        assert_eq!(remap_preset_index(&old, &current, Some(0)), None);
    }

    #[test]
    fn independent_nested_edits_are_preserved() {
        let base = json!({"panel": {"one": 1, "two": 2}, "unrelated": true});
        let edited = json!({"panel": {"one": 3, "two": 2}, "unrelated": true});
        let current = json!({"panel": {"one": 1, "two": 4}, "unrelated": false});
        assert_eq!(
            merge_value(&base, &edited, &current, false).unwrap(),
            json!({"panel": {"one": 3, "two": 4}, "unrelated": false})
        );
    }

    #[test]
    fn conflicting_removals_and_ordered_values_are_rejected() {
        let base = json!({"map": {"remove": 1, "keep": 1}, "ordered": [1, 2]});
        let edited = json!({"map": {"keep": 2}, "ordered": [2]});
        let current = json!({"map": {"remove": 3, "keep": 1, "new": 4}, "ordered": [0, 1, 2]});
        assert!(merge_value(&base, &edited, &current, false).is_err());
        let base = json!({"presets": [{"id": "a", "name": "A"}, {"id": "b", "name": "B"}]});
        let edited = json!({"presets": [{"id": "b", "name": "B"}, {"id": "a", "name": "A"}]});
        let current = json!({"presets": [{"id": "a", "name": "Edited elsewhere"}]});
        assert!(merge_value(&base, &edited, &current, false).is_err());
        assert!(merge_value(&json!(1), &json!(2), &json!(3), false).is_err());
        assert_eq!(
            merge_value(&json!(1), &json!(2), &json!(2), false).unwrap(),
            json!(2)
        );
        assert_eq!(merge_value(&base, &edited, &current, true).unwrap(), edited);
    }

    #[test]
    fn resolving_conflicts_preserves_unrelated_saved_changes() {
        let base = json!({"presets": ["a", "b"], "panel": {"visible": false}});
        let edited = json!({"presets": ["b", "a"], "panel": {"visible": false}});
        let current = json!({"presets": ["a"], "panel": {"visible": true}});
        assert!(merge_value(&base, &edited, &current, false).is_err());
        assert_eq!(
            merge_value(&base, &edited, &current, true).unwrap(),
            json!({"presets": ["b", "a"], "panel": {"visible": true}})
        );
        let untouched = json!({"presets": ["a", "b"], "panel": {"visible": true}});
        assert_eq!(
            merge_value(&base, &untouched, &current, false).unwrap(),
            current
        );
    }

    #[test]
    fn settings_edit_preserves_external_translation_and_tts_changes() {
        let base = Config::default();
        let mut edited = base.clone();
        edited.max_history_items += 1;
        let mut current = base.clone();
        current.translation_gummy.first.language = "French".into();
        current.tts_playground.draft_text = "externally edited".into();
        let merged = merge_config(&base, &edited, &current).unwrap();
        assert_eq!(merged.max_history_items, edited.max_history_items);
        assert_eq!(merged.translation_gummy.first.language, "French");
        assert_eq!(merged.tts_playground.draft_text, "externally edited");
    }

    #[test]
    fn conflict_captures_later_frame_edits_without_hiding_new_conflicts() {
        let base = Config::default();
        let mut edited = base.clone();
        edited.max_history_items += 1;
        let mut conflict = ConfigConflict {
            base: base.clone(),
            edited: edited.clone(),
        };
        edited.show_favorite_bubble = !base.show_favorite_bubble;
        conflict.capture_frame_edits(&edited);
        let mut current = base.clone();
        current.max_history_items += 2;
        assert!(merge_config(&conflict.base, &conflict.edited, &current).is_err());
        let resolved = merge_settings_edits(&conflict.base, &conflict.edited, &current).unwrap();
        assert_eq!(resolved.max_history_items, edited.max_history_items);
        assert_eq!(resolved.show_favorite_bubble, edited.show_favorite_bubble);
    }

    #[test]
    fn failed_save_retry_detects_new_conflict_in_later_frame_edit() {
        let base = Config::default();
        let mut edited = base.clone();
        edited.show_favorite_bubble = !base.show_favorite_bubble;
        let mut conflict = ConfigConflict {
            base: base.clone(),
            edited: edited.clone(),
        };
        edited.max_history_items += 1;
        conflict.capture_frame_edits(&edited);
        let mut current = base.clone();
        current.max_history_items += 2;
        assert!(merge_config(&conflict.base, &conflict.edited, &current).is_err());
    }

    #[test]
    fn favorite_toggle_and_external_bubble_geometry_both_survive_merge() {
        let base = Config::default();
        let mut edited = base.clone();
        edited.show_favorite_bubble = !base.show_favorite_bubble;
        let mut current = base.clone();
        current.favorites_keep_open = !base.favorites_keep_open;
        let merged = merge_config(&base, &edited, &current).unwrap();
        assert_eq!(merged.show_favorite_bubble, edited.show_favorite_bubble);
        assert_eq!(merged.favorites_keep_open, current.favorites_keep_open);
    }
}
