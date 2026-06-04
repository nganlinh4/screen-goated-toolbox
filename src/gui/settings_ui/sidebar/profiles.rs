use super::super::{ConfirmModal, ConfirmResult};
use super::ViewMode;
use crate::config::Config;
use crate::gui::icons::{Icon, icon_button_sized};
use crate::gui::locale::LocaleText;
use crate::gui::theme::AppTheme;
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
    let delete_confirm_id = egui::Id::new("sidebar_profile_delete_confirm_id");
    let mut editing_profile_id: Option<String> = ui.memory(|mem| mem.data.get_temp(editing_id));
    let pending_delete_id: Option<String> = ui.memory(|mem| mem.data.get_temp(delete_confirm_id));
    let can_delete = config.preset_profiles.len() > 1;

    // The "Hồ sơ" label is rendered as the first item INSIDE the scrolled pill
    // row (below), so it shares the pills' exact vertical layout and centers with
    // them. (In a separate outer cell it sat ~2px above the ScrollArea-placed
    // pills, since the scroll area positions its content independently.)
    let profiles_row_w = ui.available_width();
    ui.allocate_ui_with_layout(egui::vec2(profiles_row_w, 26.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        let theme = AppTheme::from_ui(ui);
        egui::ScrollArea::horizontal()
            .id_salt("preset_profiles_scroll")
            .max_height(30.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    // Give the section label a pill-height (22px) cell so it
                    // vertically centers ON the pills' line. A bare label here
                    // top-aligns (it's shorter than the pills and added before them),
                    // which pixel-measured 3.5px above the pills.
                    {
                        let lw = ui
                            .painter()
                            .layout_no_wrap(
                                text.profiles_label.to_string(),
                                egui::TextStyle::Body.resolve(ui.style()),
                                ui.visuals().text_color(),
                            )
                            .size()
                            .x;
                        ui.allocate_ui_with_layout(
                            egui::vec2(lw, 22.0),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.label(text.profiles_label);
                            },
                        );
                    }
                    for idx in 0..config.preset_profiles.len() {
                        let is_active = idx == config.active_preset_profile_idx;
                        let profile_id = config.preset_profiles[idx].id.clone();
                        let is_editing = editing_profile_id.as_ref() == Some(&profile_id);

                        // Each profile is a self-contained pill that carries its own
                        // rename / delete controls. Active = solid accent fill.
                        let pill_fill = if is_active {
                            theme.accent_fill()
                        } else {
                            theme.neutral_fill()
                        };
                        let on_pill = if is_active {
                            theme.on_accent()
                        } else {
                            theme.on_surface()
                        };

                        egui::Frame::new()
                            .fill(pill_fill)
                            .corner_radius(egui::CornerRadius::same(9))
                            .inner_margin(egui::Margin {
                                left: 9,
                                right: 4,
                                top: 2,
                                bottom: 2,
                            })
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 2.0;

                                    if is_editing {
                                        let response = ui.add_sized(
                                            [106.0, 20.0],
                                            egui::TextEdit::singleline(
                                                &mut config.preset_profiles[idx].name,
                                            )
                                            .desired_width(106.0),
                                        );
                                        if response.changed() {
                                            changed = true;
                                        }
                                        if response.lost_focus()
                                            || ui.input(|i| i.key_pressed(egui::Key::Enter))
                                        {
                                            if config.preset_profiles[idx].name.trim().is_empty() {
                                                config.preset_profiles[idx].name =
                                                    "Default".to_string();
                                                changed = true;
                                            }
                                            ui.memory_mut(|mem| {
                                                mem.data.remove::<String>(editing_id)
                                            });
                                            editing_profile_id = None;
                                        }
                                    } else {
                                        // Label + edit/delete icons take the pill's
                                        // on-color; the editing field above keeps the
                                        // default (readable) text color on its own
                                        // background.
                                        let widgets = &mut ui.visuals_mut().widgets;
                                        widgets.inactive.fg_stroke.color = on_pill;
                                        widgets.hovered.fg_stroke.color = on_pill;
                                        widgets.hovered.bg_fill = on_pill.gamma_multiply(0.18);

                                        let name_resp = ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(
                                                    &config.preset_profiles[idx].name,
                                                )
                                                .color(on_pill),
                                            )
                                            .selectable(false)
                                            .sense(egui::Sense::click()),
                                        );
                                        if name_resp.clicked() && !is_active {
                                            action = Some(ProfileAction::Switch(idx));
                                        }

                                        // Rename is only offered on the active profile.
                                        if is_active
                                            && icon_button_sized(ui, Icon::Edit, crate::gui::icons::ICON_MD)
                                                .on_hover_text(text.profile_edit_tooltip)
                                                .clicked()
                                        {
                                            ui.memory_mut(|mem| {
                                                mem.data
                                                    .insert_temp(editing_id, profile_id.clone())
                                            });
                                            editing_profile_id = Some(profile_id.clone());
                                        }

                                        if can_delete
                                            && icon_button_sized(ui, Icon::Close, crate::gui::icons::ICON_MD)
                                                .on_hover_text(text.profile_delete_tooltip)
                                                .clicked()
                                        {
                                            // Confirm before deleting — this also removes
                                            // every preset in the profile (irreversible).
                                            ui.memory_mut(|mem| {
                                                mem.data.remove::<String>(editing_id)
                                            });
                                            editing_profile_id = None;
                                            ui.memory_mut(|mem| {
                                                mem.data.insert_temp(
                                                    delete_confirm_id,
                                                    profile_id.clone(),
                                                )
                                            });
                                        }
                                    }
                                });
                            });
                    }

                    ui.add_space(2.0);
                    if icon_button_sized(ui, Icon::Plus, crate::gui::icons::ICON_LG)
                        .on_hover_text(text.profile_add_tooltip)
                        .clicked()
                    {
                        action = Some(ProfileAction::Add);
                    }
                });
            });
    });
    ui.add_space(2.0);

    // Confirmation dialog for the (irreversible) profile deletion.
    if let Some(pending_id) = pending_delete_id {
        match config
            .preset_profiles
            .iter()
            .position(|p| p.id == pending_id)
        {
            Some(del_idx) if can_delete => {
                let del_name = config.preset_profiles[del_idx].name.clone();
                let theme = AppTheme::from_ui(ui);
                let result = ConfirmModal::new(
                    egui::Id::new("sidebar_profile_delete_modal"),
                    text.profile_delete_confirm_title,
                    text.profile_delete_confirm_body,
                )
                .emphasis(&del_name)
                .labels(
                    text.profile_delete_confirm_yes,
                    text.profile_delete_confirm_cancel,
                )
                .destructive(true)
                .show(ui, &theme);

                match result {
                    ConfirmResult::Confirmed => {
                        action = Some(ProfileAction::Delete(del_idx));
                        ui.memory_mut(|mem| mem.data.remove::<String>(delete_confirm_id));
                    }
                    ConfirmResult::Cancelled => {
                        ui.memory_mut(|mem| mem.data.remove::<String>(delete_confirm_id));
                    }
                    ConfirmResult::Pending => {}
                }
            }
            // The target profile no longer exists (or can't be deleted) — drop stale state.
            _ => ui.memory_mut(|mem| mem.data.remove::<String>(delete_confirm_id)),
        }
    }

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
