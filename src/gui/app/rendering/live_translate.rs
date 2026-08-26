use super::super::types::SettingsApp;
use crate::gui::icons::{self, Icon};
use crate::gui::locale::LocaleText;
use crate::gui::settings_ui::{model_selector, node_graph};
use crate::gui::theme::AppTheme;
use crate::gui::widgets::{dialog_header, filled_button, removable_chip};
use crate::retry_model_chain::RetryChainKind;
use eframe::egui;

const DIALOG_WIDTH: f32 = 560.0;

impl SettingsApp {
    pub(super) fn render_live_translate_dialog(&mut self, ctx: &egui::Context, text: &LocaleText) {
        if !self.show_live_translate_dialog {
            return;
        }

        self.sync_live_translate_overlay_controls();
        let theme = AppTheme::from_dark(ctx.global_style().visuals.dark_mode);
        let active = crate::overlay::is_realtime_overlay_active();
        let mut close_requested = false;
        let mut start_stop_requested = false;
        let modal = crate::gui::widgets::material_modal(
            ctx,
            &theme,
            egui::Id::new("live_translate_dialog"),
            |ui| {
                ui.set_min_width(DIALOG_WIDTH);
                ui.set_max_width(DIALOG_WIDTH);
                let mut restore_requested = false;
                if dialog_header(
                    ui,
                    &theme,
                    text.live_translate.live_translate_title,
                    None,
                    |ui| {
                        ui.add_enabled_ui(!active, |ui| {
                            if filled_button(
                                ui,
                                text.workspace.restore_preset_btn,
                                theme.restore_fill(),
                                theme.on_accent(),
                                8,
                            )
                            .on_hover_text(text.workspace.restore_preset_tooltip)
                            .clicked()
                            {
                                restore_requested = true;
                            }
                        });
                    },
                ) {
                    close_requested = true;
                }

                if restore_requested {
                    self.restore_live_translate_defaults();
                }

                render_intro(ui, &theme, text);
                ui.add_space(8.0);
                let mut changed = false;
                ui.columns(2, |columns| {
                    columns[0].add_enabled_ui(!active, |ui| {
                        changed |= self.render_live_translate_input(ui, &theme, text);
                    });
                    columns[1].add_enabled_ui(!active, |ui| {
                        changed |= self.render_live_translate_output(ui, &theme, text);
                    });
                });
                ui.add_space(8.0);
                ui.columns(2, |columns| {
                    columns[0].add_enabled_ui(!active, |ui| {
                        changed |= self.render_live_translate_display(ui, &theme, text);
                    });
                    self.render_live_translate_hotkeys(&mut columns[1], &theme, text);
                });
                if changed {
                    self.save_and_sync();
                }
                ui.add_space(12.0);

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let label = if active {
                        text.live_translate.live_translate_stop
                    } else {
                        text.live_translate.live_translate_start
                    };
                    let fill = if active {
                        theme.danger_fill()
                    } else {
                        theme.accent_fill()
                    };
                    if filled_button(ui, label, fill, theme.on_accent(), 16).clicked() {
                        start_stop_requested = true;
                    }
                });
            },
        );

        if start_stop_requested {
            if active {
                crate::overlay::stop_realtime_overlay();
            } else {
                crate::overlay::show_realtime_overlay();
            }
            close_requested = true;
        }
        if modal.should_close() {
            close_requested = true;
        }
        if close_requested {
            self.show_live_translate_dialog = false;
            self.recording_live_translate_hotkey = false;
            self.live_translate_hotkey_conflict_msg = None;
        }
    }

    fn render_live_translate_input(
        &mut self,
        ui: &mut egui::Ui,
        theme: &AppTheme,
        text: &LocaleText,
    ) -> bool {
        let mut changed = false;
        section(
            ui,
            theme,
            text.live_translate.live_translate_input_title,
            |ui| {
                ui.horizontal(|ui| {
                    ui.label(text.preset_basics.audio_source_label);
                    crate::gui::widgets::combo("live_translate_audio_source")
                        .selected_text(if self.config.realtime_audio_source == "mic" {
                            text.preset_basics.audio_src_mic
                        } else {
                            text.preset_basics.audio_src_device
                        })
                        .show_ui(ui, |ui| {
                            changed |= ui
                                .selectable_value(
                                    &mut self.config.realtime_audio_source,
                                    "mic".to_string(),
                                    text.preset_basics.audio_src_mic,
                                )
                                .clicked();
                            changed |= ui
                                .selectable_value(
                                    &mut self.config.realtime_audio_source,
                                    "device".to_string(),
                                    text.preset_basics.audio_src_device,
                                )
                                .clicked();
                        });
                });

                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(text.realtime.realtime_tooltip_transcription_model);
                    let normalized = crate::model_config::normalize_realtime_transcription_model_id(
                        &self.config.realtime_transcription_model,
                    );
                    let selected = transcription_model_label(&normalized);
                    crate::gui::widgets::combo("live_translate_transcription_model")
                        .selected_text(selected)
                        .show_ui(ui, |ui| {
                            for &(id, label) in
                                crate::model_config::realtime_transcription_model_options()
                            {
                                changed |= ui
                                    .selectable_value(
                                        &mut self.config.realtime_transcription_model,
                                        id.to_string(),
                                        label,
                                    )
                                    .clicked();
                            }
                        });
                });

                if self.config.realtime_transcription_model == "zipformer" {
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.label(text.realtime.realtime_tooltip_transcription_language);
                        crate::gui::widgets::combo("live_translate_transcription_language")
                            .selected_text(
                                self.config.realtime_transcription_language.to_uppercase(),
                            )
                            .show_ui(ui, |ui| {
                                for (code, label) in transcription_languages() {
                                    changed |= ui
                                        .selectable_value(
                                            &mut self.config.realtime_transcription_language,
                                            code.to_string(),
                                            label,
                                        )
                                        .clicked();
                                }
                            });
                    });
                }
            },
        );
        changed
    }

    fn render_live_translate_output(
        &mut self,
        ui: &mut egui::Ui,
        theme: &AppTheme,
        text: &LocaleText,
    ) -> bool {
        let mut changed = false;
        section(
            ui,
            theme,
            text.live_translate.live_translate_translation_title,
            |ui| {
                changed |= node_graph::utils::show_language_value_selector(
                    ui,
                    text.realtime.realtime_tooltip_target_language,
                    "live_translate_target_language",
                    &mut self.config.realtime_target_language,
                );

                let is_direct_speech = crate::model_config::is_gemini_live_s2s_model_id(
                    &self.config.realtime_transcription_model,
                );
                if !is_direct_speech {
                    ui.add_space(6.0);
                    let ui_language = self.config.ui_language.clone();
                    ui.horizontal(|ui| {
                        ui.label(text.realtime.realtime_tooltip_translation_model);
                        changed |= model_selector::render_model_combo(
                            ui,
                            "live_translate_translation_model",
                            &mut self.config.realtime_translation_model,
                            RetryChainKind::TextToText,
                            &ui_language,
                        );
                    });
                }
            },
        );
        changed
    }

    fn render_live_translate_display(
        &mut self,
        ui: &mut egui::Ui,
        theme: &AppTheme,
        text: &LocaleText,
    ) -> bool {
        let mut changed = false;
        section(
            ui,
            theme,
            text.live_translate.live_translate_display_title,
            |ui| {
                ui.horizontal(|ui| {
                    ui.label(text.live_translate.live_translate_font_size);
                    changed |= ui
                        .add(egui::Slider::new(
                            &mut self.config.realtime_font_size,
                            10..=40,
                        ))
                        .changed();
                });
            },
        );
        changed
    }

    fn render_live_translate_hotkeys(
        &mut self,
        ui: &mut egui::Ui,
        theme: &AppTheme,
        text: &LocaleText,
    ) {
        let mut remove = None;
        section(
            ui,
            theme,
            text.live_translate.live_translate_hotkey_label,
            |ui| {
                ui.horizontal_wrapped(|ui| {
                    if self.recording_live_translate_hotkey {
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
                            self.recording_live_translate_hotkey = false;
                            self.live_translate_hotkey_conflict_msg = None;
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
                        self.recording_live_translate_hotkey = true;
                        self.live_translate_hotkey_conflict_msg = None;
                    }

                    if self.config.live_translate.hotkeys.is_empty() {
                        ui.label(
                            egui::RichText::new(text.live_translate.live_translate_hotkey_unset)
                                .color(theme.on_surface_variant()),
                        );
                    } else {
                        for hotkey in &self.config.live_translate.hotkeys {
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
                    }
                });
                if let Some(conflict) = &self.live_translate_hotkey_conflict_msg {
                    ui.add_space(4.0);
                    ui.colored_label(theme.danger_text(), text.hotkey_conflict_message(conflict));
                }
            },
        );

        if let Some((code, modifiers)) = remove {
            self.sync_global_hotkeys();
            if let Some(index) = self
                .config
                .live_translate
                .hotkeys
                .iter()
                .position(|hotkey| hotkey.code == code && hotkey.modifiers == modifiers)
            {
                self.config.live_translate.hotkeys.remove(index);
                self.save_and_sync();
            }
        }
    }

    fn restore_live_translate_defaults(&mut self) {
        let defaults = crate::config::Config::default();
        self.config.realtime_translation_model = defaults.realtime_translation_model;
        self.config.realtime_transcription_model = defaults.realtime_transcription_model;
        self.config.realtime_transcription_language = defaults.realtime_transcription_language;
        self.config.realtime_font_size = defaults.realtime_font_size;
        self.config.realtime_audio_source = defaults.realtime_audio_source;
        self.config.realtime_target_language = defaults.realtime_target_language;
        self.save_and_sync();
    }

    fn sync_live_translate_overlay_controls(&mut self) {
        if let Ok(state) = self.app_state_ref.lock() {
            self.config
                .sync_live_translate_overlay_controls_from(&state.config);
        }
    }
}

fn section(
    ui: &mut egui::Ui,
    theme: &AppTheme,
    title: &str,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
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
            ui.label(egui::RichText::new(title).strong());
            ui.add_space(6.0);
            add_contents(ui);
        });
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
                    ui.allocate_exact_size(egui::vec2(32.0, 32.0), egui::Sense::hover());
                ui.painter().rect_filled(
                    rect,
                    egui::CornerRadius::same(9),
                    theme.launch_live_translate(),
                );
                icons::paint_icon(
                    ui.painter(),
                    egui::Rect::from_center_size(
                        rect.center(),
                        egui::vec2(icons::ICON_XL, icons::ICON_XL),
                    ),
                    Icon::Rtt,
                    if ui.visuals().dark_mode {
                        egui::Color32::from_rgb(22, 22, 26)
                    } else {
                        egui::Color32::WHITE
                    },
                );
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(text.live_translate.live_translate_intro)
                            .size(12.5)
                            .color(theme.on_surface()),
                    )
                    .wrap(),
                );
            });
        });
}

fn transcription_model_label(model: &str) -> &'static str {
    crate::model_config::realtime_transcription_model_options()
        .iter()
        .find(|(id, _)| *id == model)
        .map(|(_, label)| *label)
        .unwrap_or("Gemini Translate")
}

fn transcription_languages() -> [(&'static str, &'static str); 8] {
    [
        ("en", "English"),
        ("ko", "Korean"),
        ("zh", "Chinese"),
        ("fr", "French"),
        ("de", "German"),
        ("es", "Spanish"),
        ("ru", "Russian"),
        ("all-8", "AR, EN, ID, JA, RU, TH, VI, ZH"),
    ]
}
