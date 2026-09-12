use super::super::types::SettingsApp;
use crate::gui::icons::{self, Icon};
use crate::gui::locale::LocaleText;
use crate::gui::settings_ui::{model_selector, node_graph};
use crate::gui::theme::AppTheme;
use crate::gui::widgets::{dialog_header, filled_button, removable_chip};
use crate::retry_model_chain::RetryChainKind;
use eframe::egui;

const DIALOG_WIDTH: f32 = 840.0;

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
                // Establish width before the header so its close button is laid out
                // against the final right edge, not the width of the title alone.
                let width = (ctx.content_rect().width() - 48.0).clamp(280.0, DIALOG_WIDTH);
                ui.set_min_width(width);
                ui.set_max_width(width);
                let mut restore_requested = false;
                if dialog_header(
                    ui,
                    &theme,
                    text.screen_translate.screen_translate_title,
                    None,
                    |ui| {
                        if filled_button(
                            ui,
                            text.screen_translate.screen_translate_restore_label,
                            theme.restore_fill(),
                            theme.on_accent(),
                            8,
                        )
                        .on_hover_text(text.screen_translate.screen_translate_restore_hint)
                        .clicked()
                        {
                            restore_requested = true;
                        }
                    },
                ) {
                    close_requested = true;
                }
                if restore_requested {
                    self.config
                        .screen_translate
                        .restore_defaults_preserving_hotkeys();
                    self.save_and_sync();
                }
                egui::ScrollArea::vertical()
                    .id_salt("screen_translate_settings_scroll")
                    .max_height((ctx.content_rect().height() - 150.0).max(180.0))
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                        render_intro(ui, &theme, text);
                        ui.add_space(8.0);
                        if ui.available_width() >= 760.0 {
                            ui.columns(2, |columns| {
                                self.render_screen_translate_output(&mut columns[0], &theme, text);
                                columns[0].add_space(8.0);
                                self.render_screen_translate_models(&mut columns[0], &theme, text);
                                self.render_screen_translate_prompt(&mut columns[1], &theme, text);
                                columns[1].add_space(8.0);
                                self.render_screen_translate_hotkeys(&mut columns[1], &theme, text);
                            });
                        } else {
                            self.render_screen_translate_output(ui, &theme, text);
                            ui.add_space(8.0);
                            self.render_screen_translate_models(ui, &theme, text);
                            ui.add_space(8.0);
                            self.render_screen_translate_prompt(ui, &theme, text);
                            ui.add_space(8.0);
                            self.render_screen_translate_hotkeys(ui, &theme, text);
                        }
                    });
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

    fn render_screen_translate_output(
        &mut self,
        ui: &mut egui::Ui,
        theme: &AppTheme,
        text: &LocaleText,
    ) {
        let mut changed = false;
        egui::Frame::new()
            .fill(theme.card_bg())
            .stroke(theme.card_stroke())
            .corner_radius(egui::CornerRadius::same(10))
            .inner_margin(egui::Margin::symmetric(
                crate::gui::theme::space::EDGE,
                crate::gui::theme::space::GAP,
            ))
            .show(ui, |ui| {
                self.render_screen_translate_language(ui, text);
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label(text.screen_translate.screen_translate_opacity_label)
                        .on_hover_text(text.screen_translate.screen_translate_opacity_hint);
                    changed = ui
                        .add(
                            egui::Slider::new(
                                &mut self.config.screen_translate.overlay_opacity,
                                10..=100,
                            )
                            .suffix("%")
                            .show_value(true),
                        )
                        .changed();
                });
            });
        if changed {
            self.save_and_sync();
        }
    }

    fn render_screen_translate_language(&mut self, ui: &mut egui::Ui, text: &LocaleText) {
        let changed = node_graph::utils::show_language_value_selector(
            ui,
            text.screen_translate.screen_translate_target_label,
            "screen_translate_target_language",
            &mut self.config.screen_translate.target_language,
        );
        if changed {
            self.save_and_sync();
        }
    }

    fn render_screen_translate_models(
        &mut self,
        ui: &mut egui::Ui,
        theme: &AppTheme,
        text: &LocaleText,
    ) {
        let mut changed = false;
        egui::Frame::new()
            .fill(theme.card_bg())
            .stroke(theme.card_stroke())
            .corner_radius(egui::CornerRadius::same(10))
            .inner_margin(egui::Margin::symmetric(
                crate::gui::theme::space::EDGE,
                crate::gui::theme::space::GAP,
            ))
            .show(ui, |ui| {
                let labels = &text.screen_translate;
                render_step(ui, theme, "1", labels.screen_translate_recognition_label)
                    .on_hover_text(format!(
                        "{}\n\n{}",
                        labels.screen_translate_recognition_hint,
                        labels.screen_translate_setup_hint
                    ));
                ui.horizontal_wrapped(|ui| {
                    render_step_number(ui, theme, "2");
                    ui.strong(labels.screen_translate_model_label)
                        .on_hover_text(labels.screen_translate_model_fallback_hint);
                    changed |= model_selector::render_model_combo(
                        ui,
                        "screen_translate_translation_model",
                        &mut self.config.screen_translate.translation_model,
                        RetryChainKind::TextToText,
                        &self.config.ui_language,
                    );
                });
                render_step(ui, theme, "3", labels.screen_translate_presentation_label)
                    .on_hover_text(labels.screen_translate_presentation_hint);
            });
        if changed {
            self.save_and_sync();
        }
    }

    fn render_screen_translate_prompt(
        &mut self,
        ui: &mut egui::Ui,
        theme: &AppTheme,
        text: &LocaleText,
    ) {
        let mut changed = false;
        egui::Frame::new()
            .fill(theme.card_bg())
            .stroke(theme.card_stroke())
            .corner_radius(egui::CornerRadius::same(10))
            .inner_margin(egui::Margin::symmetric(
                crate::gui::theme::space::EDGE,
                crate::gui::theme::space::GAP,
            ))
            .show(ui, |ui| {
                ui.strong(text.screen_translate.screen_translate_prompt_label)
                    .on_hover_text(text.screen_translate.screen_translate_prompt_hint);
                changed = node_graph::utils::show_prompt_editor(
                    ui,
                    "",
                    "",
                    &mut self.config.screen_translate.translation_prompt,
                    ui.available_width(),
                    5,
                );
            });
        if changed {
            self.save_and_sync();
        }
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
            .corner_radius(egui::CornerRadius::same(10))
            .inner_margin(egui::Margin::symmetric(
                crate::gui::theme::space::EDGE,
                crate::gui::theme::space::GAP,
            ))
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        egui::RichText::new(text.screen_translate.screen_translate_hotkey_label)
                            .strong(),
                    );
                    if self.recording_screen_translate_hotkey {
                        ui.colored_label(theme.warning(), text.preset_basics.press_keys);
                        if filled_button(
                            ui,
                            text.preset_basics.cancel_label,
                            theme.hotkey_cancel_fill(),
                            egui::Color32::WHITE,
                            10,
                        )
                        .clicked()
                        {
                            self.recording_screen_translate_hotkey = false;
                            self.screen_translate_hotkey_conflict_msg = None;
                        }
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

                    if self.config.screen_translate.hotkeys.is_empty() {
                        ui.label(
                            egui::RichText::new(
                                text.screen_translate.screen_translate_hotkey_empty,
                            )
                            .color(theme.on_surface_variant()),
                        );
                    } else {
                        for hotkey in &self.config.screen_translate.hotkeys {
                            if removable_chip(
                                ui,
                                &hotkey.display_name(),
                                theme.hotkey_item_fill(),
                                egui::Color32::WHITE,
                                10,
                            )
                            .clicked()
                            {
                                remove = Some((hotkey.code, hotkey.modifiers));
                            }
                        }
                    }
                });
                if let Some(conflict) = &self.screen_translate_hotkey_conflict_msg {
                    ui.add_space(4.0);
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

fn render_step(ui: &mut egui::Ui, theme: &AppTheme, number: &str, label: &str) -> egui::Response {
    ui.horizontal(|ui| {
        render_step_number(ui, theme, number);
        ui.add(egui::Label::new(egui::RichText::new(label).strong()).wrap());
    })
    .response
}

fn render_step_number(ui: &mut egui::Ui, theme: &AppTheme, value: &str) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, egui::CornerRadius::same(7), theme.neutral_fill());
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        value,
        egui::TextStyle::Button.resolve(ui.style()),
        theme.on_surface(),
    );
}

fn render_intro(ui: &mut egui::Ui, theme: &AppTheme, text: &LocaleText) {
    egui::Frame::new()
        .fill(theme.neutral_fill())
        .corner_radius(egui::CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(
            crate::gui::theme::space::EDGE,
            crate::gui::theme::space::GAP,
        ))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(30.0, 30.0), egui::Sense::hover());
                ui.painter().rect_filled(
                    rect,
                    egui::CornerRadius::same(9),
                    theme.launch_screen_translate(),
                );
                icons::paint_icon(
                    ui.painter(),
                    egui::Rect::from_center_size(
                        rect.center(),
                        egui::vec2(icons::ICON_LG, icons::ICON_LG),
                    ),
                    Icon::Translate,
                    if ui.visuals().dark_mode {
                        egui::Color32::from_rgb(22, 22, 26)
                    } else {
                        egui::Color32::WHITE
                    },
                );
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(text.screen_translate.screen_translate_intro)
                            .size(12.5)
                            .color(theme.on_surface()),
                    )
                    .wrap(),
                );
            });
        });
}
