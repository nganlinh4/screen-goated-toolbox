use crate::config::{Config, ModelPriorityChains};
use crate::gui::locale::LocaleText;
use crate::gui::theme::AppTheme;
use crate::model_config::{
    ModelConfig, ModelType, get_all_models_with_ollama, get_model_by_id, model_is_non_llm,
    model_supports_search_by_id,
};
use crate::retry_model_chain::RetryChainKind;
use eframe::egui;

pub fn render_model_priority_modal(
    ui: &mut egui::Ui,
    config: &mut Config,
    text: &LocaleText,
    show_modal: &mut bool,
) -> bool {
    if !*show_modal {
        return false;
    }

    let theme = AppTheme::from_ui(ui);
    let mut changed = false;

    let modal = egui::Modal::new(egui::Id::new("model_priority_modal"))
        .backdrop_color(theme.scrim_color())
        .frame(theme.dialog_frame())
        .show(ui.ctx(), |ui| {
            ui.set_width(760.0);

            // Header: title + skip-hint description + close.
            if crate::gui::widgets::dialog_header(
                ui,
                &theme,
                text.model_priority_title,
                Some(text.model_priority_skip_hint),
                |_| {},
            ) {
                *show_modal = false;
            }

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

    if modal.should_close() {
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
    let theme = AppTheme::from_ui(ui);
    let section_title_color = match chain_kind {
        RetryChainKind::ImageToText => theme.node_special_title(),
        RetryChainKind::TextToText => theme.on_surface(),
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

                let selected_text = model_short_label(&chain[row_idx], ui_language);
                egui::ComboBox::from_id_salt((section_id, "combo", row_idx))
                    .selected_text(selected_text)
                    .width(240.0)
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

fn provider_icon(provider: &str) -> &'static str {
    match provider {
        "google" | "gemini-live" => "✨ ",
        "google-gtx" => "🌍 ",
        "groq" => "⚡ ",
        "cerebras" => "🔥 ",
        "openrouter" => "🌐 ",
        "ollama" => "🏠 ",
        "qrserver" => "🔳 ",
        "parakeet" => "🐦 ",
        "qwen3" => "● ",
        "taalas" => "🚀 ",
        _ => "⚙️ ",
    }
}

/// Full label shown inside the expanded dropdown list: icon + friendly name +
/// model id + quota (+ search badge).
fn model_option_label(model: &crate::model_config::ModelConfig, ui_language: &str) -> String {
    let search_suffix = if model_supports_search_by_id(&model.id) {
        " 🔍"
    } else {
        ""
    };
    let name = localized_model_name(model, ui_language);
    let quota = localized_quota(model, ui_language);

    format!(
        "{}{} - {} - {}{}",
        provider_icon(&model.provider),
        name,
        model.full_name,
        quota,
        search_suffix
    )
}

/// Compact label for the collapsed dropdown button: icon + friendly name
/// (+ search badge) only. Keeps every row the same width so the reorder
/// controls don't drift — full details stay in the expanded list.
fn model_short_label(model_id: &str, ui_language: &str) -> String {
    get_model_by_id(model_id)
        .map(|model| {
            let search_suffix = if model_supports_search_by_id(&model.id) {
                " 🔍"
            } else {
                ""
            };
            format!(
                "{}{}{}",
                provider_icon(&model.provider),
                localized_model_name(&model, ui_language),
                search_suffix
            )
        })
        .unwrap_or_else(|| model_id.to_string())
}
