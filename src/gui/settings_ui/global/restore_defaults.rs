use crate::config::{Config, RestoreDefaultsSelection};
use crate::gui::icons::{Icon, draw_icon_static};
use crate::gui::locale::LocaleText;
use crate::gui::settings_ui::node_graph::request_node_graph_view_reset;
use crate::gui::theme::{AppTheme, blend};
use auto_launch::AutoLaunch;
use eframe::egui;

pub(super) fn render_restore_defaults_modal(
    ui: &mut egui::Ui,
    config: &mut Config,
    text: &LocaleText,
    show_modal: &mut bool,
    run_at_startup: &mut bool,
    auto_launcher: &Option<AutoLaunch>,
) -> bool {
    if !*show_modal {
        return false;
    }

    let theme = AppTheme::from_ui(ui);
    let mut selection_changed = false;
    let mut close_requested = false;
    let mut restore_requested = false;

    let modal = crate::gui::widgets::material_modal(
        ui.ctx(),
        &theme,
        egui::Id::new("restore_defaults_modal"),
        |ui| {
            // The settings window is at least 1245 px wide. Spending a little
            // more horizontal space keeps the category explanations on one
            // line and prevents the dialog from crowding the 660 px min height.
            ui.set_width(560.0);

            if crate::gui::widgets::dialog_header(
                ui,
                &theme,
                text.global_settings.restore_defaults_title,
                None,
                |_| {},
            ) {
                close_requested = true;
            }

            ui.add(
                egui::Label::new(
                    egui::RichText::new(text.global_settings.restore_defaults_description)
                        .size(12.5)
                        .color(theme.on_surface_variant()),
                )
                .wrap(),
            );
            ui.add_space(10.0);

            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let selection = &mut config.restore_defaults_selection;
                    if selection.any()
                        && ui
                            .small_button(text.global_settings.restore_defaults_clear_all)
                            .clicked()
                    {
                        selection.set_all(false);
                        selection_changed = true;
                    }
                    if !selection.all()
                        && ui
                            .small_button(text.global_settings.restore_defaults_select_all)
                            .clicked()
                    {
                        selection.set_all(true);
                        selection_changed = true;
                    }
                });
            });
            ui.add_space(6.0);

            let selection = &mut config.restore_defaults_selection;
            selection_changed |= category_row(
                ui,
                &theme,
                &mut selection.presets,
                text.global_settings.restore_defaults_presets_title,
                text.global_settings.restore_defaults_presets_description,
                false,
            );
            selection_changed |= category_row(
                ui,
                &theme,
                &mut selection.app_settings,
                text.global_settings.restore_defaults_app_title,
                text.global_settings.restore_defaults_app_description,
                false,
            );
            selection_changed |= category_row(
                ui,
                &theme,
                &mut selection.model_settings,
                text.global_settings.restore_defaults_models_title,
                text.global_settings.restore_defaults_models_description,
                false,
            );
            selection_changed |= category_row(
                ui,
                &theme,
                &mut selection.audio_settings,
                text.global_settings.restore_defaults_audio_title,
                text.global_settings.restore_defaults_audio_description,
                false,
            );
            selection_changed |= category_row(
                ui,
                &theme,
                &mut selection.shortcuts_and_mini_apps,
                text.global_settings.restore_defaults_shortcuts_title,
                text.global_settings.restore_defaults_shortcuts_description,
                false,
            );
            selection_changed |= category_row(
                ui,
                &theme,
                &mut selection.local_data,
                text.global_settings.restore_defaults_data_title,
                text.global_settings.restore_defaults_data_description,
                true,
            );

            ui.add_space(8.0);
            egui::Frame::new()
                .fill(ui.visuals().faint_bg_color)
                .corner_radius(8.0)
                .inner_margin(egui::Margin::symmetric(
                    crate::gui::theme::space::EDGE,
                    crate::gui::theme::space::GAP,
                ))
                .show(ui, |ui| {
                    ui.horizontal_top(|ui| {
                        draw_icon_static(ui, Icon::Key, Some(crate::gui::icons::ICON_SM));
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(
                                    text.global_settings.restore_defaults_kept_note,
                                )
                                .size(11.5)
                                .color(theme.on_surface_variant()),
                            )
                            .wrap(),
                        );
                    });
                });

            ui.add_space(12.0);
            if selection.local_data {
                ui.horizontal(|ui| {
                    draw_icon_static(ui, Icon::Warning, Some(crate::gui::icons::ICON_SM));
                    ui.label(
                        egui::RichText::new(text.global_settings.restore_defaults_data_warning)
                            .size(11.5)
                            .strong()
                            .color(theme.danger_text()),
                    );
                });
                ui.add_space(8.0);
            } else if !selection.any() {
                ui.label(
                    egui::RichText::new(text.global_settings.restore_defaults_empty)
                        .size(11.5)
                        .color(theme.warning()),
                );
                ui.add_space(8.0);
            }

            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    let confirm_fill = if selection.local_data {
                        theme.danger_fill()
                    } else {
                        theme.accent_fill()
                    };
                    let confirm = ui
                        .add_enabled_ui(selection.any(), |ui| {
                            crate::gui::widgets::filled_button(
                                ui,
                                text.global_settings.restore_defaults_confirm,
                                confirm_fill,
                                theme.on_accent(),
                                16,
                            )
                        })
                        .inner;
                    if confirm.clicked() {
                        restore_requested = true;
                    }
                    if crate::gui::widgets::filled_button(
                        ui,
                        text.global_settings.restore_defaults_cancel,
                        theme.neutral_fill(),
                        theme.on_surface(),
                        16,
                    )
                    .clicked()
                    {
                        close_requested = true;
                    }
                });
            });
        },
    );

    #[cfg(test)]
    ui.ctx().data_mut(|data| {
        data.insert_temp(
            egui::Id::new("restore_defaults_modal_test_rect"),
            modal.response.rect,
        );
    });

    if modal.should_close() {
        close_requested = true;
    }
    if close_requested {
        *show_modal = false;
    }

    if restore_requested {
        let selection = config.restore_defaults_selection;
        restore_selected(ui, config, selection, run_at_startup, auto_launcher);
        *show_modal = false;
        return true;
    }

    selection_changed
}

fn category_row(
    ui: &mut egui::Ui,
    theme: &AppTheme,
    selected: &mut bool,
    title: &str,
    description: &str,
    destructive: bool,
) -> bool {
    let selected_accent = if destructive {
        theme.danger_fill()
    } else {
        theme.accent_fill()
    };
    let fill = if *selected {
        blend(theme.dialog_surface(), selected_accent, 0.08)
    } else {
        ui.visuals().faint_bg_color
    };
    let stroke_color = if *selected {
        blend(theme.on_surface_variant(), selected_accent, 0.55)
    } else {
        theme.card_stroke().color
    };

    let changed = egui::Frame::new()
        .fill(fill)
        .stroke(egui::Stroke::new(1.0, stroke_color))
        .corner_radius(9.0)
        .inner_margin(egui::Margin::symmetric(
            crate::gui::theme::space::EDGE,
            crate::gui::theme::space::SNUG,
        ))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.vertical(|ui| {
                let response = ui.checkbox(
                    selected,
                    egui::RichText::new(title).size(13.0).strong().color(
                        if destructive && *selected {
                            theme.danger_text()
                        } else {
                            theme.on_surface()
                        },
                    ),
                );
                ui.horizontal_top(|ui| {
                    ui.add_space(ui.spacing().icon_width + ui.spacing().item_spacing.x);
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(description)
                                .size(11.5)
                                .color(theme.on_surface_variant()),
                        )
                        .wrap(),
                    );
                });
                response.changed()
            })
            .inner
        })
        .inner;
    ui.add_space(4.0);
    changed
}

fn restore_selected(
    ui: &egui::Ui,
    config: &mut Config,
    selection: RestoreDefaultsSelection,
    run_at_startup: &mut bool,
    auto_launcher: &Option<AutoLaunch>,
) {
    if selection.app_settings {
        disable_startup(auto_launcher);
        *run_at_startup = false;
    }

    let defaults = Config::default();
    apply_selected_config_defaults_from(config, selection, &defaults);

    if selection.presets {
        request_node_graph_view_reset(ui.ctx());
    }

    // Persist the new config before deleting history/media, keeping the window
    // between destructive cleanup and process exit as short as possible.
    crate::config::save_config(config);
    if selection.local_data {
        crate::overlay::clear_all_app_data();
    }

    crate::gui::app::restart_app();
}

fn disable_startup(auto_launcher: &Option<AutoLaunch>) {
    if crate::gui::utils::is_admin_startup_enabled() && !crate::gui::utils::set_admin_startup(false)
    {
        crate::log_info!("[restore-defaults] failed to disable admin startup task");
    }
    if let Some(launcher) = auto_launcher
        && launcher.is_enabled().unwrap_or(false)
        && let Err(error) = launcher.disable()
    {
        crate::log_info!("[restore-defaults] failed to disable startup entry: {error}");
    }
}

fn apply_selected_config_defaults_from(
    config: &mut Config,
    selection: RestoreDefaultsSelection,
    defaults: &Config,
) {
    if selection.presets {
        config.restore_presets_and_profiles_preserving_user_state(defaults);
    }

    if selection.app_settings {
        config.theme_mode = defaults.theme_mode.clone();
        config.max_history_items = defaults.max_history_items;
        config.max_screen_record_projects = defaults.max_screen_record_projects;
        config.max_screen_record_recent_uploads = defaults.max_screen_record_recent_uploads;
        config.cc_max_memory_items = defaults.cc_max_memory_items;
        config.graphics_mode = defaults.graphics_mode.clone();
        config.favorite_overlay_opacity = defaults.favorite_overlay_opacity;
        config.start_in_tray = defaults.start_in_tray;
        config.show_startup_animation = defaults.show_startup_animation;
        config.run_as_admin_on_startup = defaults.run_as_admin_on_startup;
        config.run_at_startup = defaults.run_at_startup;
        config.authorized_startup_path = defaults.authorized_startup_path.clone();
        config.show_favorite_bubble = defaults.show_favorite_bubble;
        config.favorite_bubble_position = defaults.favorite_bubble_position;
        config.favorites_keep_open = defaults.favorites_keep_open;
        config.favorite_bubble_size = defaults.favorite_bubble_size;
    }

    if selection.model_settings {
        config.model_priority_chains = defaults.model_priority_chains.clone();
        config.ollama_vision_model = defaults.ollama_vision_model.clone();
        config.ollama_text_model = defaults.ollama_text_model.clone();
    }

    if selection.audio_settings {
        config.realtime_translation_model = defaults.realtime_translation_model.clone();
        config.realtime_transcription_model = defaults.realtime_transcription_model.clone();
        config.realtime_transcription_language = defaults.realtime_transcription_language.clone();
        config.realtime_font_size = defaults.realtime_font_size;
        config.realtime_transcription_size = defaults.realtime_transcription_size;
        config.realtime_translation_size = defaults.realtime_translation_size;
        config.realtime_audio_source = defaults.realtime_audio_source.clone();
        config.realtime_target_language = defaults.realtime_target_language.clone();
        config.tts_method = defaults.tts_method.clone();
        config.tts_voice = defaults.tts_voice.clone();
        config.tts_speed = defaults.tts_speed.clone();
        config.tts_gemini_live_model = defaults.tts_gemini_live_model.clone();
        config.tts_output_device = defaults.tts_output_device.clone();
        config.tts_language_conditions = defaults.tts_language_conditions.clone();
        config.edge_tts_settings = defaults.edge_tts_settings.clone();
        config.step_audio_settings = defaults.step_audio_settings.clone();
        config.step_audio_reference_voices = defaults.step_audio_reference_voices.clone();
        config.magpie_settings = defaults.magpie_settings.clone();
        config.kokoro_settings = defaults.kokoro_settings.clone();
        config.supertonic_settings = defaults.supertonic_settings.clone();
        config.vieneu_settings = defaults.vieneu_settings.clone();
        config.voxtral_settings = defaults.voxtral_settings.clone();
        config.tts_playground = defaults.tts_playground.clone();
    }

    if selection.shortcuts_and_mini_apps {
        config.screen_record_window_size = defaults.screen_record_window_size;
        let screen_translate_hotkeys = config.screen_translate.hotkeys.clone();
        config.screen_translate = defaults.screen_translate.clone();
        config.screen_translate.hotkeys = screen_translate_hotkeys;
        let legacy_hotkey = config.translation_gummy.hotkey.clone();
        let hotkeys = config.translation_gummy.hotkeys.clone();
        config.translation_gummy = defaults.translation_gummy.clone();
        config.translation_gummy.hotkey = legacy_hotkey;
        config.translation_gummy.hotkeys = hotkeys;
    }

    if selection.local_data {
        config.clear_webview_on_startup = true;
    }
}

#[cfg(test)]
#[path = "restore_defaults_tests.rs"]
mod tests;
