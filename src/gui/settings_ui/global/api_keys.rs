use crate::config::Config;
use crate::gui::icons::{Icon, draw_icon_static, icon_button};
use crate::gui::locale::LocaleText;
use eframe::egui;

/// A key row: the field claims every point left after the row's trailing
/// control, so the eye stays pinned to the right edge at any card width.
///
/// The fields used to be a fixed 400pt, which left dead space between the key
/// and its eye on a wide card and would have overflowed a narrow one. Laying
/// the row out right-to-left puts the trailing control first and hands the
/// field whatever remains, which needs no width constant at all.
fn secret_row(ui: &mut egui::Ui, id: &str, value: &mut String, visible: &mut bool) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let eye = if *visible {
                Icon::EyeOpen
            } else {
                Icon::EyeClosed
            };
            if icon_button(ui, eye)
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                *visible = !*visible;
            }
            let width = ui.available_width();
            changed = ui
                .add(
                    egui::TextEdit::singleline(value)
                        .id(egui::Id::new(id))
                        .password(!*visible)
                        .desired_width(width),
                )
                .changed();
        });
    });
    changed
}

/// Same geometry for the plain URL row, whose trailing slot holds a status
/// label instead of a toggle.
fn url_row(ui: &mut egui::Ui, id: &str, value: &mut String, status: Option<String>) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if let Some(status) = status {
                ui.label(egui::RichText::new(status).size(11.0));
            }
            let width = ui.available_width();
            changed = ui
                .add(
                    egui::TextEdit::singleline(value)
                        .id(egui::Id::new(id))
                        .desired_width(width),
                )
                .changed();
        });
    });
    changed
}

pub(super) struct ApiKeyVisibility<'a> {
    pub(super) groq: &'a mut bool,
    pub(super) gemini: &'a mut bool,
    pub(super) openrouter: &'a mut bool,
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
    } = visibility;
    let mut changed = false;
    egui::Frame::new()
        .fill(style.background)
        .stroke(style.stroke)
        .inner_margin(12.0)
        .corner_radius(10.0)
        .show(ui, |ui| {
            // Fill the (now wider) panel so the card doesn't leave a blank strip
            // beside it that reads as a gap before the next column.
            ui.set_min_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                draw_icon_static(ui, Icon::Key, Some(crate::gui::icons::ICON_MD));
                ui.label(
                    egui::RichText::new(text.global_settings.api_keys_header)
                        .strong()
                        .size(14.0),
                );
                ui.add_space(16.0);

                if ui
                    .checkbox(&mut config.use_groq, text.preset_basics.use_groq_checkbox)
                    .changed()
                {
                    changed = true;
                }
                if ui
                    .checkbox(
                        &mut config.use_gemini,
                        text.preset_basics.use_gemini_checkbox,
                    )
                    .changed()
                {
                    changed = true;
                }
                if ui
                    .checkbox(
                        &mut config.use_openrouter,
                        text.preset_basics.use_openrouter_checkbox,
                    )
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
                    ui.label(text.global_settings.groq_label);
                    if ui.link(text.preset_basics.get_key_link).clicked() {
                        let _ = open::that("https://console.groq.com/keys");
                    }
                });
                if secret_row(ui, "settings_api_key_groq", &mut config.api_key, groq) {
                    changed = true;
                }
            }

            if config.use_gemini {
                ui.horizontal(|ui| {
                    ui.label(text.preset_basics.gemini_api_key_label);
                    if ui.link(text.preset_basics.gemini_get_key_link).clicked() {
                        let _ = open::that("https://aistudio.google.com/app/apikey");
                    }
                });
                if secret_row(
                    ui,
                    "settings_api_key_gemini",
                    &mut config.gemini_api_key,
                    gemini,
                ) {
                    changed = true;
                }
            }

            if config.use_openrouter {
                ui.horizontal(|ui| {
                    ui.label(text.preset_basics.openrouter_api_key_label);
                    if ui
                        .link(text.preset_basics.openrouter_get_key_link)
                        .clicked()
                    {
                        let _ = open::that("https://openrouter.ai/settings/keys");
                    }
                });
                if secret_row(
                    ui,
                    "settings_api_key_openrouter",
                    &mut config.openrouter_api_key,
                    openrouter,
                ) {
                    changed = true;
                }
            }

            if config.use_ollama {
                ui.horizontal(|ui| {
                    ui.label("Ollama URL:");
                    if ui.link(text.global_settings.ollama_url_guide).clicked() {
                        let _ = open::that("https://docs.ollama.com/api/introduction#base-url");
                    }
                });
                let status = ui
                    .ctx()
                    .memory(|mem| mem.data.get_temp::<String>(egui::Id::new("ollama_status")));
                if url_row(
                    ui,
                    "settings_api_key_ollama_url",
                    &mut config.ollama_base_url,
                    status,
                ) {
                    changed = true;
                }
            }
        });
    changed
}
