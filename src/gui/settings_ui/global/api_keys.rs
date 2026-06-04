use crate::config::Config;
use crate::gui::icons::{Icon, draw_icon_static, icon_button};
use crate::gui::locale::LocaleText;
use eframe::egui;

const API_KEY_FIELD_WIDTH: f32 = 400.0;

pub(super) struct ApiKeyVisibility<'a> {
    pub(super) groq: &'a mut bool,
    pub(super) gemini: &'a mut bool,
    pub(super) openrouter: &'a mut bool,
    pub(super) cerebras: &'a mut bool,
}

pub(super) struct ApiKeyCardStyle {
    pub(super) background: egui::Color32,
    pub(super) stroke: egui::Stroke,
}

pub(super) fn render_api_keys_card(
    ui: &mut egui::Ui,
    config: &mut Config,
    visibility: ApiKeyVisibility<'_>,
    text: &LocaleText,
    style: ApiKeyCardStyle,
) -> bool {
    let ApiKeyVisibility {
        groq,
        gemini,
        openrouter,
        cerebras,
    } = visibility;
    let mut changed = false;
    egui::Frame::new()
        .fill(style.background)
        .stroke(style.stroke)
        .inner_margin(12.0)
        .corner_radius(10.0)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                draw_icon_static(ui, Icon::Key, Some(crate::gui::icons::ICON_MD));
                ui.label(
                    egui::RichText::new(text.api_keys_header)
                        .strong()
                        .size(14.0),
                );
                ui.add_space(16.0);

                if ui
                    .checkbox(&mut config.use_groq, text.use_groq_checkbox)
                    .changed()
                {
                    changed = true;
                }
                if ui
                    .checkbox(&mut config.use_cerebras, text.use_cerebras_checkbox)
                    .changed()
                {
                    changed = true;
                }
                if ui
                    .checkbox(&mut config.use_gemini, text.use_gemini_checkbox)
                    .changed()
                {
                    changed = true;
                }
                if ui
                    .checkbox(&mut config.use_openrouter, text.use_openrouter_checkbox)
                    .changed()
                {
                    changed = true;
                }
                if ui.checkbox(&mut config.use_ollama, "Ollama").changed() {
                    changed = true;
                }
            });
            ui.add_space(6.0);

            if config.use_groq {
                ui.horizontal(|ui| {
                    ui.label(text.groq_label);
                    if ui.link(text.get_key_link).clicked() {
                        let _ = open::that("https://console.groq.com/keys");
                    }
                });
                ui.horizontal(|ui| {
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut config.api_key)
                                .id(egui::Id::new("settings_api_key_groq"))
                                .password(!*groq)
                                .desired_width(API_KEY_FIELD_WIDTH),
                        )
                        .changed()
                    {
                        changed = true;
                    }
                    let eye_icon = if *groq {
                        Icon::EyeOpen
                    } else {
                        Icon::EyeClosed
                    };
                    if icon_button(ui, eye_icon).clicked() {
                        *groq = !*groq;
                    }
                });
            }

            if config.use_cerebras {
                ui.horizontal(|ui| {
                    ui.label(text.cerebras_api_key_label);
                    if ui.link(text.cerebras_get_key_link).clicked() {
                        let _ = open::that("https://cloud.cerebras.ai/");
                    }
                });
                ui.horizontal(|ui| {
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut config.cerebras_api_key)
                                .id(egui::Id::new("settings_api_key_cerebras"))
                                .password(!*cerebras)
                                .desired_width(API_KEY_FIELD_WIDTH),
                        )
                        .changed()
                    {
                        changed = true;
                    }
                    let eye_icon = if *cerebras {
                        Icon::EyeOpen
                    } else {
                        Icon::EyeClosed
                    };
                    if icon_button(ui, eye_icon).clicked() {
                        *cerebras = !*cerebras;
                    }
                });
            }

            if config.use_gemini {
                ui.horizontal(|ui| {
                    ui.label(text.gemini_api_key_label);
                    if ui.link(text.gemini_get_key_link).clicked() {
                        let _ = open::that("https://aistudio.google.com/app/apikey");
                    }
                });
                ui.horizontal(|ui| {
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut config.gemini_api_key)
                                .id(egui::Id::new("settings_api_key_gemini"))
                                .password(!*gemini)
                                .desired_width(API_KEY_FIELD_WIDTH),
                        )
                        .changed()
                    {
                        changed = true;
                    }
                    let eye_icon = if *gemini {
                        Icon::EyeOpen
                    } else {
                        Icon::EyeClosed
                    };
                    if icon_button(ui, eye_icon).clicked() {
                        *gemini = !*gemini;
                    }
                });
            }

            if config.use_openrouter {
                ui.horizontal(|ui| {
                    ui.label(text.openrouter_api_key_label);
                    if ui.link(text.openrouter_get_key_link).clicked() {
                        let _ = open::that("https://openrouter.ai/settings/keys");
                    }
                });
                ui.horizontal(|ui| {
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut config.openrouter_api_key)
                                .id(egui::Id::new("settings_api_key_openrouter"))
                                .password(!*openrouter)
                                .desired_width(API_KEY_FIELD_WIDTH),
                        )
                        .changed()
                    {
                        changed = true;
                    }
                    let eye_icon = if *openrouter {
                        Icon::EyeOpen
                    } else {
                        Icon::EyeClosed
                    };
                    if icon_button(ui, eye_icon).clicked() {
                        *openrouter = !*openrouter;
                    }
                });
            }

            if config.use_ollama {
                ui.horizontal(|ui| {
                    ui.label("Ollama URL:");
                    if ui.link(text.ollama_url_guide).clicked() {
                        let _ = open::that("https://docs.ollama.com/api/introduction#base-url");
                    }
                });
                ui.horizontal(|ui| {
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut config.ollama_base_url)
                                .id(egui::Id::new("settings_api_key_ollama_url"))
                                .desired_width(API_KEY_FIELD_WIDTH),
                        )
                        .changed()
                    {
                        changed = true;
                    }
                    if let Some(status) = ui
                        .ctx()
                        .memory(|mem| mem.data.get_temp::<String>(egui::Id::new("ollama_status")))
                    {
                        ui.label(egui::RichText::new(&status).size(11.0));
                    }
                });
            }
        });
    changed
}
