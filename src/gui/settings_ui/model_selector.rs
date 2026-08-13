//! Shared text/vision model selector used by preset fallback and mini-app flows.

use crate::gui::icons::{Icon, draw_icon_static, provider_icon};
use crate::model_config::{
    ModelConfig, ModelType, get_all_models_with_ollama, get_model_by_id, model_is_non_llm,
    model_search_tool_enabled_by_default_by_id,
};
use crate::retry_model_chain::RetryChainKind;
use eframe::egui;

pub(crate) const MODEL_SELECTOR_WIDTH: f32 = 240.0 - crate::gui::model_performance::PREFIX_WIDTH;

pub(crate) fn compatible_models(chain_kind: RetryChainKind) -> Vec<ModelConfig> {
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

pub(crate) fn render_model_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    model_id: &mut String,
    chain_kind: RetryChainKind,
    ui_language: &str,
) -> bool {
    let models = compatible_models(chain_kind);
    let selected = get_model_by_id(model_id);
    if let Some(model) = selected.as_ref() {
        crate::gui::model_performance::render_prefix(ui, model);
        draw_icon_static(
            ui,
            provider_icon(&model.provider),
            Some(crate::gui::icons::ICON_MD),
        );
    } else {
        crate::gui::model_performance::render_unknown_prefix(ui);
    }

    let mut changed = false;
    crate::gui::widgets::combo(id)
        .selected_text(model_short_label(model_id, ui_language))
        .width(MODEL_SELECTOR_WIDTH)
        .show_ui(ui, |ui| {
            for model in &models {
                ui.horizontal(|ui| {
                    crate::gui::model_performance::render_prefix(ui, model);
                    draw_icon_static(
                        ui,
                        provider_icon(&model.provider),
                        Some(crate::gui::icons::ICON_MD),
                    );
                    if ui
                        .selectable_label(
                            *model_id == model.id,
                            model_option_label(model, ui_language),
                        )
                        .clicked()
                    {
                        model_id.clone_from(&model.id);
                        changed = true;
                    }
                    if model_search_tool_enabled_by_default_by_id(&model.id) {
                        draw_icon_static(ui, Icon::Search, Some(crate::gui::icons::ICON_XS));
                    }
                });
            }
        });
    changed
}

pub(crate) fn default_model_id(chain_kind: RetryChainKind) -> String {
    compatible_models(chain_kind)
        .first()
        .map(|model| model.id.clone())
        .unwrap_or_default()
}

fn model_option_label(model: &ModelConfig, ui_language: &str) -> String {
    format!(
        "{} - {} - {}",
        model.localized_name(ui_language),
        model.full_name,
        model.localized_quota(ui_language)
    )
}

fn model_short_label(model_id: &str, ui_language: &str) -> String {
    get_model_by_id(model_id)
        .map(|model| model.localized_name(ui_language).to_string())
        .unwrap_or_else(|| model_id.to_string())
}
