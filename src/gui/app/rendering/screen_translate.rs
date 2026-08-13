use super::super::types::SettingsApp;
use crate::gui::icons::{self, Icon};
use crate::gui::locale::LocaleText;
use crate::gui::theme::AppTheme;
use crate::gui::widgets::{dialog_header, filled_button, removable_chip};
use eframe::egui;

impl SettingsApp {
    pub(super) fn render_screen_translate_dialog(
        &mut self,
        ctx: &egui::Context,
        text: &LocaleText,
    ) {
        if !self.show_screen_translate_dialog {
            return;
        }
        let theme = AppTheme::from_dark(ctx.global_style().visuals.dark_mode);
        let mut close_requested = false;
        let modal = crate::gui::widgets::material_modal(
            ctx,
            &theme,
            egui::Id::new("screen_translate_dialog"),
            |ui| {
                ui.set_width(430.0);
                if dialog_header(
                    ui,
                    &theme,
                    text.screen_translate.screen_translate_title,
                    None,
                    |_| {},
                ) {
                    close_requested = true;
                }
                render_intro(ui, &theme, text);
                ui.add_space(12.0);
                self.render_screen_translate_language(ui, &theme, text);
                ui.add_space(10.0);
                self.render_screen_translate_hotkeys(ui, &theme, text);
            },
        );
        if modal.should_close() {
            close_requested = true;
        }
        if close_requested {
            self.show_screen_translate_dialog = false;
            self.recording_screen_translate_hotkey = false;
            self.screen_translate_hotkey_conflict_msg = None;
        }
    }

    fn render_screen_translate_language(
        &mut self,
        ui: &mut egui::Ui,
        theme: &AppTheme,
        text: &LocaleText,
    ) {
        egui::Frame::new()
            .fill(theme.card_bg())
            .stroke(theme.card_stroke())
            .corner_radius(egui::CornerRadius::same(12))
            .inner_margin(egui::Margin::same(12))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(text.screen_translate.screen_translate_target_label)
                            .strong()
                            .color(theme.on_surface()),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let before = self.config.screen_translate.target_language.clone();
                        egui::ComboBox::from_id_salt("screen_translate_target_language")
                            .selected_text(&self.config.screen_translate.target_language)
                            .width(190.0)
                            .show_ui(ui, |ui| {
                                for language in crate::config::get_all_languages() {
                                    ui.selectable_value(
                                        &mut self.config.screen_translate.target_language,
                                        language.clone(),
                                        language,
                                    );
                                }
                            });
                        if self.config.screen_translate.target_language != before {
                            self.save_and_sync();
                        }
                    });
                });
            });
    }

    fn render_screen_translate_hotkeys(
        &mut self,
        ui: &mut egui::Ui,
        theme: &AppTheme,
        text: &LocaleText,
    ) {
        let mut remove = None;
        egui::Frame::new()
            .fill(theme.card_bg())
            .stroke(theme.card_stroke())
            .corner_radius(egui::CornerRadius::same(12))
            .inner_margin(egui::Margin::same(12))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(text.screen_translate.screen_translate_hotkey_label)
                            .strong()
                            .color(theme.on_surface()),
                    );
                    if self.recording_screen_translate_hotkey {
                        ui.colored_label(theme.warning(), text.preset_basics.press_keys);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if filled_button(
                                ui,
                                text.preset_basics.cancel_label,
                                theme.hotkey_cancel_fill(),
                                egui::Color32::WHITE,
                                12,
                            )
                            .clicked()
                            {
                                self.recording_screen_translate_hotkey = false;
                                self.screen_translate_hotkey_conflict_msg = None;
                            }
                        });
                    } else if filled_button(
                        ui,
                        text.preset_basics.add_hotkey_button,
                        theme.hotkey_add_fill(),
                        egui::Color32::WHITE,
                        10,
                    )
                    .clicked()
                    {
                        self.recording_screen_translate_hotkey = true;
                        self.screen_translate_hotkey_conflict_msg = None;
                    }
                });
                ui.add_space(8.0);
                if self.config.screen_translate.hotkeys.is_empty() {
                    ui.label(
                        egui::RichText::new(text.screen_translate.screen_translate_hotkey_empty)
                            .color(theme.on_surface_variant()),
                    );
                } else {
                    ui.horizontal_wrapped(|ui| {
                        for hotkey in &self.config.screen_translate.hotkeys {
                            if removable_chip(
                                ui,
                                &hotkey.name,
                                theme.hotkey_item_fill(),
                                egui::Color32::WHITE,
                                10,
                            )
                            .clicked()
                            {
                                remove = Some((hotkey.code, hotkey.modifiers));
                            }
                        }
                    });
                }
                if let Some(conflict) = &self.screen_translate_hotkey_conflict_msg {
                    ui.add_space(6.0);
                    ui.colored_label(theme.danger_text(), text.hotkey_conflict_message(conflict));
                }
            });
        if let Some((code, modifiers)) = remove {
            self.sync_global_hotkeys();
            if let Some(index) = self
                .config
                .screen_translate
                .hotkeys
                .iter()
                .position(|hotkey| hotkey.code == code && hotkey.modifiers == modifiers)
            {
                self.config.screen_translate.hotkeys.remove(index);
                self.save_and_sync();
            }
        }
    }
}

fn render_intro(ui: &mut egui::Ui, theme: &AppTheme, text: &LocaleText) {
    egui::Frame::new()
        .fill(theme.neutral_fill())
        .corner_radius(egui::CornerRadius::same(12))
        .inner_margin(egui::Margin::same(14))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(34.0, 34.0), egui::Sense::hover());
                ui.painter().rect_filled(
                    rect,
                    egui::CornerRadius::same(10),
                    theme.launch_translation(),
                );
                icons::paint_icon(
                    ui.painter(),
                    egui::Rect::from_center_size(
                        rect.center(),
                        egui::vec2(icons::ICON_XL, icons::ICON_XL),
                    ),
                    Icon::Translate,
                    if ui.visuals().dark_mode {
                        egui::Color32::from_rgb(22, 22, 26)
                    } else {
                        egui::Color32::WHITE
                    },
                );
                ui.label(
                    egui::RichText::new(text.screen_translate.screen_translate_intro)
                        .size(13.0)
                        .color(theme.on_surface()),
                );
            });
        });
}
