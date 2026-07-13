use crate::config::types::CustomModelDefinition;
use crate::gui::locale::LocaleText;
use crate::gui::theme::AppTheme;
use crate::model_config::get_all_models_with_custom;
use eframe::egui;
use std::collections::HashMap;

#[expect(
    clippy::too_many_arguments,
    reason = "modal rendering consumes distinct provider toggles and shared UI state"
)]
pub fn render_usage_modal(
    ui: &mut egui::Ui,
    usage_stats: &HashMap<String, String>,
    text: &LocaleText,
    show_modal: &mut bool,
    use_groq: bool,
    use_gemini: bool,
    use_openrouter: bool,
    use_ollama: bool,
    use_cerebras: bool,
    custom_models: &[CustomModelDefinition],
) {
    if !*show_modal {
        return;
    }

    let theme = AppTheme::from_ui(ui);

    let modal = egui::Modal::new(egui::Id::new("usage_statistics_modal"))
        .backdrop_color(theme.scrim_color())
        .frame(theme.dialog_frame())
        .show(ui.ctx(), |ui| {
            ui.set_width(400.0);

            if crate::gui::widgets::dialog_header(
                ui,
                &theme,
                text.desktop_settings.usage_statistics_title,
                None,
                |_| {},
            ) {
                *show_modal = false;
            }

            let all_models = get_all_models_with_custom(custom_models);

            let mut shown_models = std::collections::HashSet::new();

            egui::ScrollArea::vertical()
                .max_height(450.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                ui.set_width(ui.available_width());
                if use_groq {
                    let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(
                        ui.ctx(),
                        egui::Id::new("usage_groq_header"),
                        true,
                    );
                    let header = ui.horizontal(|ui| {
                        state.show_toggle_button(ui, crate::gui::widgets::collapsing_chevron);
                        crate::gui::icons::draw_icon_static(ui, crate::gui::icons::provider_icon("groq"), Some(crate::gui::icons::ICON_MD));
                        ui.label(egui::RichText::new("Groq").strong().size(13.0));
                    });
                    state.show_body_indented(&header.response, ui, |ui| {
                        egui::Grid::new("groq_grid").striped(true).show(ui, |ui| {
                            ui.label(egui::RichText::new(text.desktop_settings.usage_model_column).strong().size(11.0));
                            ui.label(egui::RichText::new(text.desktop_settings.usage_remaining_column).strong().size(11.0));
                            ui.end_row();

                            for model in &all_models {
                                if !model.enabled || model.provider != "groq" { continue; }
                                if shown_models.contains(&model.full_name) { continue; }
                                shown_models.insert(model.full_name.clone());

                                ui.label(&model.full_name);

                                if model.model_type == crate::model_config::ModelType::Audio {
                                    ui.label("");
                                    ui.end_row();
                                    continue;
                                }

                                let static_limit = model.quota_limit_en.split_whitespace().next().unwrap_or("?");
                                let default_status = format!("??? / {}", static_limit);

                                let raw_status = usage_stats.get(&model.full_name).cloned().unwrap_or(default_status);
                                let display_status = if let Some((usage, limit)) = raw_status.split_once(" / ") {
                                    let final_limit = if limit == "?" { static_limit } else { limit };
                                    format!("{} / {}", usage, final_limit)
                                } else {
                                    raw_status
                                };

                                ui.label(display_status);
                                ui.end_row();
                            }

                        });
                    });
                }

                if use_cerebras {
                    let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(
                        ui.ctx(),
                        egui::Id::new("usage_cerebras_header"),
                        true,
                    );
                    let header = ui.horizontal(|ui| {
                        state.show_toggle_button(ui, crate::gui::widgets::collapsing_chevron);
                        crate::gui::icons::draw_icon_static(ui, crate::gui::icons::provider_icon("cerebras"), Some(crate::gui::icons::ICON_MD));
                        ui.label(egui::RichText::new("Cerebras").strong().size(13.0));
                    });
                    state.show_body_indented(&header.response, ui, |ui| {
                        egui::Grid::new("cerebras_grid").striped(true).show(ui, |ui| {
                            ui.label(egui::RichText::new(text.desktop_settings.usage_model_column).strong().size(11.0));
                            ui.label(egui::RichText::new(text.desktop_settings.usage_remaining_column).strong().size(11.0));
                            ui.end_row();

                            for model in &all_models {
                                if !model.enabled || model.provider != "cerebras" { continue; }
                                if shown_models.contains(&model.full_name) { continue; }
                                shown_models.insert(model.full_name.clone());

                                ui.label(&model.full_name);

                                let static_limit = model.quota_limit_en.split_whitespace().next().unwrap_or("?");
                                let default_status = format!("??? / {}", static_limit);

                                let raw_status = usage_stats.get(&model.full_name).cloned().unwrap_or(default_status);
                                let display_status = if let Some((usage, limit)) = raw_status.split_once(" / ") {
                                    let final_limit = if limit == "?" { static_limit } else { limit };
                                    format!("{} / {}", usage, final_limit)
                                } else {
                                    raw_status
                                };

                                ui.label(display_status);
                                ui.end_row();
                            }

                            let realtime_model =
                                crate::model_config::DEFAULT_TEXT_API_MODEL;
                            if !shown_models.contains(realtime_model) {
                                shown_models.insert(realtime_model.to_string());
                                ui.label(realtime_model);
                                let static_limit = "14400";
                                let default_status = format!("??? / {}", static_limit);
                                let raw_status = usage_stats
                                    .get(realtime_model)
                                    .cloned()
                                    .unwrap_or(default_status);
                                let display_status = if let Some((usage, limit)) = raw_status.split_once(" / ") {
                                    let final_limit = if limit == "?" { static_limit } else { limit };
                                    format!("{} / {}", usage, final_limit)
                                } else {
                                    raw_status
                                };
                                ui.label(display_status);
                                ui.end_row();
                            }
                        });
                        ui.add_space(4.0);
                        ui.hyperlink_to(text.desktop_settings.usage_check_link, "https://cloud.cerebras.ai/");
                    });
                }

                if use_gemini {
                    let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(
                        ui.ctx(),
                        egui::Id::new("usage_gemini_header"),
                        true,
                    );
                    let header = ui.horizontal(|ui| {
                        state.show_toggle_button(ui, crate::gui::widgets::collapsing_chevron);
                        crate::gui::icons::draw_icon_static(ui, crate::gui::icons::provider_icon("google"), Some(crate::gui::icons::ICON_MD));
                        ui.label(egui::RichText::new("Google Gemini").strong().size(13.0));
                    });
                    state.show_body_indented(&header.response, ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(text.desktop_settings.usage_model_column).strong().size(11.0));
                            ui.add_space(120.0);
                            ui.hyperlink_to(text.desktop_settings.usage_check_link, "https://aistudio.google.com/usage?timeRange=last-1-day&tab=rate-limit");
                        });
                        ui.add_space(4.0);

                        for model in &all_models {
                            if !model.enabled || model.provider != "google" { continue; }
                            if shown_models.contains(&model.full_name) { continue; }
                            shown_models.insert(model.full_name.clone());

                            ui.label(&model.full_name);
                        }
                    });
                }

                if use_openrouter {
                    let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(
                        ui.ctx(),
                        egui::Id::new("usage_openrouter_header"),
                        true,
                    );
                    let header = ui.horizontal(|ui| {
                        state.show_toggle_button(ui, crate::gui::widgets::collapsing_chevron);
                        crate::gui::icons::draw_icon_static(ui, crate::gui::icons::provider_icon("openrouter"), Some(crate::gui::icons::ICON_MD));
                        ui.label(egui::RichText::new("OpenRouter").strong().size(13.0));
                    });
                    state.show_body_indented(&header.response, ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(text.desktop_settings.usage_model_column).strong().size(11.0));
                            ui.add_space(120.0);
                            ui.hyperlink_to(text.desktop_settings.usage_check_link, "https://openrouter.ai/activity");
                        });
                        ui.add_space(4.0);

                        for model in &all_models {
                            if !model.enabled || model.provider != "openrouter" { continue; }
                            if shown_models.contains(&model.full_name) { continue; }
                            shown_models.insert(model.full_name.clone());

                            ui.label(&model.full_name);
                        }
                    });
                }

                if use_ollama {
                    let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(
                        ui.ctx(),
                        egui::Id::new("usage_ollama_header"),
                        true,
                    );
                    let header = ui.horizontal(|ui| {
                        state.show_toggle_button(ui, crate::gui::widgets::collapsing_chevron);
                        crate::gui::icons::draw_icon_static(ui, crate::gui::icons::provider_icon("ollama"), Some(crate::gui::icons::ICON_MD));
                        ui.label(egui::RichText::new("Ollama (Local)").strong().size(13.0));
                    });
                    state.show_body_indented(&header.response, ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(text.desktop_settings.usage_model_column).strong().size(11.0));
                            ui.add_space(120.0);
                            ui.label(text.overlay.unlimited_label);
                        });
                        ui.add_space(4.0);

                        for model in &all_models {
                            if !model.enabled || model.provider != "ollama" { continue; }
                            if shown_models.contains(&model.full_name) { continue; }
                            shown_models.insert(model.full_name.clone());

                            ui.label(&model.full_name);
                        }
                    });
                }
            });
        });

    if modal.should_close() {
        *show_modal = false;
    }
}
