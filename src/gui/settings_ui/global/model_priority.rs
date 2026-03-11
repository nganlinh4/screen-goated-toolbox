use crate::config::{Config, ModelPriorityChains};
use crate::gui::locale::LocaleText;
use crate::model_config::{
    ModelConfig, ModelType, get_all_models_with_ollama, get_model_by_id, model_is_non_llm,
    model_supports_search_by_id,
};
use crate::retry_model_chain::RetryChainKind;
use eframe::egui;

pub fn render_model_priority_modal(
    ctx: &egui::Context,
    config: &mut Config,
    text: &LocaleText,
    show_modal: &mut bool,
) -> bool {
    if !*show_modal {
        return false;
    }

    let mut changed = false;
    let mut open = true;

    egui::Window::new(text.model_priority_title)
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(760.0)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.label(egui::RichText::new(text.model_priority_skip_hint).small());
            ui.add_space(8.0);

            ui.columns(2, |columns| {
                if render_chain_section(
                    &mut columns[0],
                    &mut config.model_priority_chains.image_to_text,
                    RetryChainKind::ImageToText,
                    &config.ui_language,
                    text,
                ) {
                    changed = true;
                }

                if render_chain_section(
                    &mut columns[1],
                    &mut config.model_priority_chains.text_to_text,
                    RetryChainKind::TextToText,
                    &config.ui_language,
                    text,
                ) {
                    changed = true;
                }
            });
        });

    if !open {
        *show_modal = false;
    }

    changed
}

fn render_chain_section(
    ui: &mut egui::Ui,
    chain: &mut Vec<String>,
    chain_kind: RetryChainKind,
    ui_language: &str,
    text: &LocaleText,
) -> bool {
    enum RowAction {
        None,
        MoveUp,
        MoveDown,
        Remove,
    }

    let mut changed = false;
    let section_title = match chain_kind {
        RetryChainKind::ImageToText => text.model_priority_image_chain_title,
        RetryChainKind::TextToText => text.model_priority_text_chain_title,
    };
    let section_id = match chain_kind {
        RetryChainKind::ImageToText => "model_priority_image_chain",
        RetryChainKind::TextToText => "model_priority_text_chain",
    };
    let available_models = compatible_models(chain_kind);
    let section_title_color = match chain_kind {
        RetryChainKind::ImageToText => {
            if ui.visuals().dark_mode {
                egui::Color32::from_rgb(255, 200, 100)
            } else {
                egui::Color32::from_rgb(200, 100, 0)
            }
        }
        RetryChainKind::TextToText => ui.visuals().text_color(),
    };

    ui.group(|ui| {
        ui.set_min_width(340.0);
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(section_title)
                    .strong()
                    .size(13.0)
                    .color(section_title_color),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button(text.reset_defaults_btn).clicked() {
                    let defaults = ModelPriorityChains::default();
                    *chain = match chain_kind {
                        RetryChainKind::ImageToText => defaults.image_to_text,
                        RetryChainKind::TextToText => defaults.text_to_text,
                    };
                    changed = true;
                }
            });
        });
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.label("1.");
            ui.label(egui::RichText::new(text.model_priority_chosen_model).strong());
            ui.label(egui::RichText::new("→").weak());
            ui.label(
                egui::RichText::new(text.model_priority_fixed_hint)
                    .small()
                    .weak(),
            );
        });
        ui.add_space(6.0);

        let mut row_idx = 0;
        while row_idx < chain.len() {
            let mut row_action = RowAction::None;
            ui.horizontal(|ui| {
                ui.label(format!("{}.", row_idx + 2));

                let selected_text = model_label(&chain[row_idx], ui_language);
                egui::ComboBox::from_id_salt((section_id, "combo", row_idx))
                    .selected_text(selected_text)
                    .width(260.0)
                    .show_ui(ui, |ui| {
                        for model in &available_models {
                            let label = model_option_label(model, ui_language);
                            if ui
                                .selectable_label(chain[row_idx] == model.id, label)
                                .clicked()
                            {
                                chain[row_idx] = model.id.clone();
                                changed = true;
                            }
                        }
                    });

                if ui.small_button("↑").clicked() && row_idx > 0 {
                    row_action = RowAction::MoveUp;
                }
                if ui.small_button("↓").clicked() && row_idx + 1 < chain.len() {
                    row_action = RowAction::MoveDown;
                }
                if ui.small_button("×").clicked() {
                    row_action = RowAction::Remove;
                }
            });

            match row_action {
                RowAction::MoveUp => {
                    chain.swap(row_idx, row_idx - 1);
                    changed = true;
                    row_idx = row_idx.saturating_sub(1);
                }
                RowAction::MoveDown => {
                    chain.swap(row_idx, row_idx + 1);
                    changed = true;
                    row_idx += 1;
                }
                RowAction::Remove => {
                    chain.remove(row_idx);
                    changed = true;
                    continue;
                }
                RowAction::None => {}
            }

            row_idx += 1;
        }

        ui.add_space(4.0);
        if ui.button(text.model_priority_add_model).clicked() {
            chain.push(default_insert_model_id(&available_models));
            changed = true;
        }

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(format!("{}.", chain.len() + 2));
            ui.label(egui::RichText::new(text.model_priority_auto).strong());
            ui.label(egui::RichText::new("→").weak());
            ui.label(
                egui::RichText::new(text.model_priority_auto_hint)
                    .small()
                    .weak(),
            );
        });
    });

    changed
}

fn compatible_models(chain_kind: RetryChainKind) -> Vec<crate::model_config::ModelConfig> {
    let model_type = match chain_kind {
        RetryChainKind::ImageToText => ModelType::Vision,
        RetryChainKind::TextToText => ModelType::Text,
    };

    get_all_models_with_ollama()
        .into_iter()
        .filter(|model| {
            model.enabled && model.model_type == model_type && !model_is_non_llm(&model.id)
        })
        .collect()
}

fn default_insert_model_id(models: &[crate::model_config::ModelConfig]) -> String {
    models
        .first()
        .map(|model| model.id.clone())
        .unwrap_or_default()
}

fn model_label(model_id: &str, ui_language: &str) -> String {
    get_model_by_id(model_id)
        .map(|model| model_option_label(&model, ui_language))
        .unwrap_or_else(|| model_id.to_string())
}

fn localized_model_name<'a>(model: &'a ModelConfig, ui_language: &str) -> &'a str {
    match ui_language {
        "vi" => &model.name_vi,
        "ko" => &model.name_ko,
        _ => &model.name_en,
    }
}

fn localized_quota<'a>(model: &'a ModelConfig, ui_language: &str) -> &'a str {
    match ui_language {
        "vi" => &model.quota_limit_vi,
        "ko" => &model.quota_limit_ko,
        _ => &model.quota_limit_en,
    }
}

fn model_option_label(model: &crate::model_config::ModelConfig, ui_language: &str) -> String {
    let provider_icon = match model.provider.as_str() {
        "google" | "gemini-live" => "✨ ",
        "google-gtx" => "🌍 ",
        "groq" => "⚡ ",
        "cerebras" => "🔥 ",
        "openrouter" => "🌐 ",
        "ollama" => "🏠 ",
        "qrserver" => "🔳 ",
        "parakeet" => "🐦 ",
        _ => "⚙️ ",
    };
    let search_suffix = if model_supports_search_by_id(&model.id) {
        " 🔍"
    } else {
        ""
    };
    let name = localized_model_name(model, ui_language);
    let quota = localized_quota(model, ui_language);

    format!(
        "{}{} - {} - {}{}",
        provider_icon, name, model.full_name, quota, search_suffix
    )
}
