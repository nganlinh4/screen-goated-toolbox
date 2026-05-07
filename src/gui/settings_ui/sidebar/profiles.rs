use super::ViewMode;
use crate::config::Config;
use crate::gui::icons::{Icon, icon_button_sized};
use crate::gui::locale::LocaleText;
use eframe::egui;

enum ProfileAction {
    Switch(usize),
    Add,
    Delete(usize),
}

pub fn render_profiles(
    ui: &mut egui::Ui,
    config: &mut Config,
    view_mode: &mut ViewMode,
    text: &LocaleText,
) -> bool {
    if config.preset_profiles.is_empty() {
        config.sync_active_profile_from_presets();
    }

    let mut changed = false;
    let mut action = None;
    let editing_id = egui::Id::new("sidebar_profile_editing_id");
    let mut editing_profile_id: Option<String> = ui.memory(|mem| mem.data.get_temp(editing_id));
    let can_delete = config.preset_profiles.len() > 1;

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.label(text.profiles_label);
        egui::ScrollArea::horizontal()
            .id_salt("preset_profiles_scroll")
            .max_height(24.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    for idx in 0..config.preset_profiles.len() {
                        let is_active = idx == config.active_preset_profile_idx;
                        let profile_id = config.preset_profiles[idx].id.clone();
                        let is_editing = editing_profile_id.as_ref() == Some(&profile_id);

                        if is_editing {
                            let response = ui.add_sized(
                                [106.0, 20.0],
                                egui::TextEdit::singleline(&mut config.preset_profiles[idx].name)
                                    .desired_width(106.0),
                            );
                            if response.changed() {
                                changed = true;
                            }
                            if response.lost_focus()
                                || ui.input(|i| i.key_pressed(egui::Key::Enter))
                            {
                                if config.preset_profiles[idx].name.trim().is_empty() {
                                    config.preset_profiles[idx].name = "Default".to_string();
                                    changed = true;
                                }
                                ui.memory_mut(|mem| mem.data.remove::<String>(editing_id));
                                editing_profile_id = None;
                            }
                        } else if ui
                            .selectable_label(is_active, &config.preset_profiles[idx].name)
                            .clicked()
                            && !is_active
                        {
                            action = Some(ProfileAction::Switch(idx));
                        }

                        if is_active
                            && !is_editing
                            && icon_button_sized(ui, Icon::Edit, 18.0)
                                .on_hover_text(text.profile_edit_tooltip)
                                .clicked()
                        {
                            ui.memory_mut(|mem| {
                                mem.data.insert_temp(editing_id, profile_id.clone())
                            });
                            editing_profile_id = Some(profile_id);
                        }

                        if can_delete
                            && icon_button_sized(ui, Icon::Close, 18.0)
                                .on_hover_text(text.profile_delete_tooltip)
                                .clicked()
                        {
                            action = Some(ProfileAction::Delete(idx));
                        }
                    }

                    ui.add_space(2.0);
                    if icon_button_sized(ui, Icon::Plus, 20.0)
                        .on_hover_text(text.profile_add_tooltip)
                        .clicked()
                    {
                        action = Some(ProfileAction::Add);
                    }
                });
            });
    });
    ui.add_space(2.0);

    if let Some(action) = action {
        if let ViewMode::Preset(idx) = *view_mode {
            config.active_preset_idx = idx.min(config.presets.len().saturating_sub(1));
        }

        match action {
            ProfileAction::Switch(idx) => config.switch_preset_profile(idx),
            ProfileAction::Add => config.add_preset_profile_from_active(),
            ProfileAction::Delete(idx) => config.delete_preset_profile(idx),
        }

        *view_mode = if config.presets.is_empty() {
            ViewMode::Global
        } else {
            ViewMode::Preset(config.active_preset_idx.min(config.presets.len() - 1))
        };
        crate::overlay::favorite_bubble::update_favorites_panel();
        ui.memory_mut(|mem| mem.data.remove::<String>(editing_id));
        changed = true;
    }

    changed
}
