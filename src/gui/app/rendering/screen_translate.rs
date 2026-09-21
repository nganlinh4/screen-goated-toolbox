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
                let content_height = (ctx.content_rect().height() - 48.0).max(1.0);
                ui.set_max_height(content_height);
                let header_top = ui.cursor().top();
                let mut restore_requested = false;
                let header_closed = ui
                    .scope(|ui| {
                        let control_height = ui.spacing().interact_size.y;
                        if width >= 760.0 {
                            ui.spacing_mut().interact_size.y = control_height.max(24.0);
                        }
                        dialog_header(
                            ui,
                            &theme,
                            text.screen_translate.screen_translate_title,
                            None,
                            |ui| {
                                ui.spacing_mut().interact_size.y = control_height;
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
                                if width >= 760.0 {
                                    ui.add_space(8.0);
                                    render_intro(ui, &theme, text);
                                }
                            },
                        )
                    })
                    .inner;
                if header_closed {
                    close_requested = true;
                }
                if width < 760.0 {
                    render_intro(ui, &theme, text);
                    ui.add_space(8.0);
                }
                if restore_requested {
                    self.config
                        .screen_translate
                        .restore_defaults_preserving_hotkeys();
                    self.save_and_sync();
                }
                egui::ScrollArea::vertical()
                    .id_salt("screen_translate_settings_scroll")
                    .max_height((content_height - (ui.cursor().top() - header_top)).max(1.0))
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                        if ui.available_width() >= 760.0 {
                            ui.columns(2, |columns| {
                                self.render_screen_translate_output(&mut columns[0], &theme, text);
                                columns[0].add_space(8.0);
                                self.render_screen_translate_models(&mut columns[0], &theme, text);
                                let height = columns[0].min_rect().height();
                                self.render_screen_translate_prompt(
                                    &mut columns[1],
                                    &theme,
                                    text,
                                    height,
                                );
                            });
                            ui.add_space(8.0);
                            ui.columns(2, |columns| {
                                self.render_screen_translate_hotkey_row(
                                    &mut columns[0],
                                    &theme,
                                    text,
                                    0,
                                );
                                self.render_screen_translate_hotkey_row(
                                    &mut columns[1],
                                    &theme,
                                    text,
                                    1,
                                );
                            });
                        } else {
                            self.render_screen_translate_output(ui, &theme, text);
                            ui.add_space(8.0);
                            self.render_screen_translate_models(ui, &theme, text);
                            ui.add_space(8.0);
                            self.render_screen_translate_prompt(ui, &theme, text, 0.0);
                            ui.add_space(8.0);
                            self.render_screen_translate_hotkeys(ui, &theme, text);
                        }
                        ui.add_space(8.0);
                        self.render_screen_translate_hotkey_row(ui, &theme, text, 2);
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
                ui.set_min_width(ui.available_width());
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
                changed |= ui
                    .checkbox(
                        &mut self.config.screen_translate.show_glow_box,
                        text.screen_translate.screen_translate_show_glow_box,
                    )
                    .changed();
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
                ui.set_min_width(ui.available_width());
                let labels = &text.screen_translate;
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(labels.screen_translate_recognition_label).strong(),
                    )
                    .wrap(),
                )
                .on_hover_text(format!(
                    "{}\n\n{}",
                    labels.screen_translate_recognition_hint, labels.screen_translate_setup_hint
                ));
                ui.horizontal_wrapped(|ui| {
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
        height: f32,
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
                ui.set_min_width(ui.available_width());
                ui.set_min_height(
                    (height - 2.0 * f32::from(crate::gui::theme::space::GAP)).max(0.0),
                );
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
        self.render_screen_translate_hotkey_row(ui, theme, text, 0);
        ui.add_space(8.0);
        self.render_screen_translate_hotkey_row(ui, theme, text, 1);
    }

    fn render_screen_translate_hotkey_row(
        &mut self,
        ui: &mut egui::Ui,
        theme: &AppTheme,
        text: &LocaleText,
        mode: u8,
    ) {
        let fullscreen = mode == 1;
        let subtitles = mode == 2;
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
                ui.set_min_width(ui.available_width());
                ui.set_min_height(48.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        egui::RichText::new(if subtitles {
                            text.screen_translate.screen_translate_subtitle_mode
                        } else if fullscreen {
                            text.screen_translate
                                .screen_translate_fullscreen_hotkey_label
                        } else {
                            text.screen_translate.screen_translate_hotkey_label
                        })
                        .strong(),
                    );
                    if self.recording_screen_translate_hotkey
                        && self.recording_screen_translate_fullscreen == fullscreen
                        && self.recording_screen_translate_subtitles == subtitles
                    {
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
                        self.recording_screen_translate_fullscreen = fullscreen;
                        self.recording_screen_translate_subtitles = subtitles;
                        self.screen_translate_hotkey_conflict_msg = None;
                    }

                    let hotkeys = if subtitles {
                        &self.config.screen_translate.subtitle_hotkeys
                    } else if fullscreen {
                        &self.config.screen_translate.fullscreen_hotkeys
                    } else {
                        &self.config.screen_translate.hotkeys
                    };
                    if hotkeys.is_empty() {
                        ui.label(
                            egui::RichText::new(
                                text.screen_translate.screen_translate_hotkey_empty,
                            )
                            .color(theme.on_surface_variant()),
                        );
                    } else {
                        for hotkey in hotkeys {
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
                if fullscreen
                    && crate::gui::widgets::filled_trailing_icon_button(
                        ui,
                        text.screen_translate.screen_translate_adjust_region,
                        Icon::ArrowRight,
                        theme.neutral_fill(),
                        theme.on_surface(),
                        8,
                    )
                    .on_hover_text(text.screen_translate.screen_translate_fullscreen_hint)
                    .clicked()
                {
                    self.start_screen_translate_region_edit(ui.ctx());
                }
                if self.recording_screen_translate_fullscreen == fullscreen
                    && self.recording_screen_translate_subtitles == subtitles
                    && let Some(conflict) = &self.screen_translate_hotkey_conflict_msg
                {
                    ui.add_space(4.0);
                    ui.colored_label(theme.danger_text(), text.hotkey_conflict_message(conflict));
                }
            });
        if let Some((code, modifiers)) = remove {
            self.sync_global_hotkeys();
            let hotkeys = if subtitles {
                &mut self.config.screen_translate.subtitle_hotkeys
            } else if fullscreen {
                &mut self.config.screen_translate.fullscreen_hotkeys
            } else {
                &mut self.config.screen_translate.hotkeys
            };
            if let Some(index) = hotkeys
                .iter()
                .position(|hotkey| hotkey.code == code && hotkey.modifiers == modifiers)
            {
                hotkeys.remove(index);
                self.save_and_sync();
            }
        }
    }
}

fn render_intro(ui: &mut egui::Ui, theme: &AppTheme, text: &LocaleText) {
    // Reserve room for the header's close button when sharing its row.
    let width = (ui.available_width() - 40.0).max(120.0);
    ui.scope(|ui| {
        ui.set_max_width(width);
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
            icons::paint_icon(
                ui.painter(),
                rect,
                Icon::Translate,
                theme.launch_screen_translate(),
            );
            ui.add(
                egui::Label::new(
                    egui::RichText::new(text.screen_translate.screen_translate_intro)
                        .size(crate::gui::widgets::DIALOG_DESCRIPTION_SIZE)
                        .color(theme.on_surface_variant()),
                )
                .wrap(),
            );
        });
    });
}
