//! Shared text/vision model selector used by preset fallback and mini-app flows.

use crate::gui::icons::{Icon, draw_icon_static, provider_icon};
use crate::model_config::{
    ModelConfig, ModelType, get_all_models_with_custom, get_all_models_with_ollama,
    model_is_non_llm, sort_models_for_display,
};
use crate::retry_model_chain::RetryChainKind;
use eframe::egui;

pub(crate) const MODEL_SELECTOR_WIDTH: f32 = 240.0 - crate::gui::model_performance::PREFIX_WIDTH;
pub(crate) const MODEL_POPUP_MAX_HEIGHT: f32 = 360.0;

pub(crate) fn model_popup_scroll<R>(
    ui: &mut egui::Ui,
    contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    egui::ScrollArea::vertical()
        .max_height(MODEL_POPUP_MAX_HEIGHT)
        .auto_shrink([false, false])
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
        .show(ui, contents)
        .inner
}

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

/// Builds one selector catalog from the config already borrowed by the caller.
///
/// Dense editors must prepare this once and share it across their rows. Calling
/// [`compatible_models`] from every row repeatedly locks global app state,
/// clones the dynamic catalog, and sorts it even while every popup is closed.
pub(crate) fn selector_models(
    custom_models: &[crate::config::types::CustomModelDefinition],
) -> Vec<ModelConfig> {
    let mut models = get_all_models_with_custom(custom_models);
    sort_models_for_display(&mut models);
    models
}

pub(crate) fn render_model_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    model_id: &mut String,
    chain_kind: RetryChainKind,
    ui_language: &str,
) -> bool {
    let models = compatible_models(chain_kind);
    render_model_combo_from_models(ui, id, model_id, chain_kind, ui_language, &models)
}

pub(crate) fn render_model_combo_from_models(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    model_id: &mut String,
    chain_kind: RetryChainKind,
    ui_language: &str,
    models: &[ModelConfig],
) -> bool {
    let selected = models.iter().find(|model| model.id == *model_id);
    if let Some(model) = selected {
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
    let button = ui
        .push_id(id, |ui| {
            ui.add_sized(
                egui::vec2(MODEL_SELECTOR_WIDTH, ui.spacing().interact_size.y),
                egui::Button::new(model_short_label_from_models(models, model_id, ui_language))
                    .truncate(),
            )
        })
        .inner;
    let icon_width = ui.spacing().icon_width;
    let icon_rect = egui::Rect::from_center_size(
        egui::pos2(
            button.rect.right() - ui.spacing().button_padding.x - icon_width * 0.5,
            button.rect.center().y,
        ),
        egui::vec2(icon_width, icon_width),
    );
    crate::gui::icons::paint_icon(
        ui.painter(),
        icon_rect,
        Icon::ArrowDown,
        ui.style().interact(&button).fg_stroke.color,
    );
    if button.clicked() {
        egui::Popup::toggle_id(ui.ctx(), button.id);
    }
    let popup_id = button.id;
    egui::Popup::from_toggle_button_response(&button)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            ui.set_min_width(460.0);
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
            model_popup_scroll(ui, |ui| {
                for model in models
                    .iter()
                    .filter(|model| compatible_with_chain(model, chain_kind))
                {
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
                            egui::Popup::toggle_id(ui.ctx(), popup_id);
                        }
                        if model.search_tool_enabled_by_default {
                            draw_icon_static(ui, Icon::Search, Some(crate::gui::icons::ICON_XS));
                        }
                    });
                }
            });
        });
    changed
}

pub(crate) fn default_model_id_from_models(
    models: &[ModelConfig],
    chain_kind: RetryChainKind,
) -> String {
    models
        .iter()
        .find(|model| compatible_with_chain(model, chain_kind))
        .map(|model| model.id.clone())
        .unwrap_or_default()
}

fn compatible_with_chain(model: &ModelConfig, chain_kind: RetryChainKind) -> bool {
    let model_type = match chain_kind {
        RetryChainKind::ImageToText => ModelType::Vision,
        RetryChainKind::TextToText => ModelType::Text,
    };
    model.enabled && model.model_type == model_type && !model_is_non_llm(&model.id)
}

fn model_option_label(model: &ModelConfig, ui_language: &str) -> String {
    format!(
        "{} - {} - {}",
        model.localized_name(ui_language),
        model.full_name,
        model.localized_quota(ui_language)
    )
}

pub(crate) fn model_short_label_from_models(
    models: &[ModelConfig],
    model_id: &str,
    ui_language: &str,
) -> String {
    models
        .iter()
        .find(|model| model.id == model_id)
        .map(|model| model.localized_name(ui_language).to_string())
        .unwrap_or_else(|| model_id.to_string())
}
