use super::style::{accent_icon, on_color, provider_accent};
use super::{custom_model_type_label, unique_custom_id};
use crate::config::Config;
use crate::config::types::{CustomModelDefinition, CustomModelType};
use crate::gui::icons::{self, Icon};
use crate::gui::locale::LocaleText;
use crate::gui::theme::AppTheme;
use crate::gui::widgets::filled_icon_button;
use eframe::egui;
use serde::Deserialize;
use std::sync::{LazyLock, Mutex};

#[derive(Clone, Debug)]
pub(super) struct OpenRouterImportModel {
    id: String,
    name: String,
    model_type: CustomModelType,
}

#[derive(Default)]
struct OpenRouterImportState {
    loading: bool,
    error: Option<String>,
    models: Vec<OpenRouterImportModel>,
}

#[derive(Deserialize)]
struct OpenRouterModelsResponse {
    #[serde(default)]
    data: Vec<serde_json::Value>,
}

static OPENROUTER_IMPORT: LazyLock<Mutex<OpenRouterImportState>> =
    LazyLock::new(|| Mutex::new(OpenRouterImportState::default()));

pub(super) fn render_openrouter_import_results(
    ui: &mut egui::Ui,
    config: &mut Config,
    text: &LocaleText,
    changed: &mut bool,
) {
    let Ok(state) = OPENROUTER_IMPORT.lock() else {
        return;
    };
    if state.loading {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(text.model_catalog.custom_models_import_openrouter);
        });
        return;
    }
    if let Some(error) = &state.error {
        ui.colored_label(ui.visuals().error_fg_color, error);
    }
    if state.models.is_empty() {
        return;
    }

    let models = state.models.clone();
    drop(state);
    let theme = AppTheme::from_ui(ui);
    let dark = ui.visuals().dark_mode;
    ui.horizontal(|ui| {
        accent_icon(
            ui,
            Icon::Public,
            provider_accent("openrouter", dark),
            icons::ICON_MD,
        );
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(format!("OpenRouter · {} models", models.len()))
                .color(theme.on_surface_variant()),
        );
        ui.add_space(8.0);
        let select_label = match config.ui_language.as_str() {
            "vi" => "Chọn mô hình...",
            "ko" => "모델 선택...",
            _ => "Select model...",
        };
        let openrouter_accent = provider_accent("openrouter", dark);
        let button = filled_icon_button(
            ui,
            Icon::Plus,
            select_label,
            openrouter_accent,
            on_color(openrouter_accent),
            8,
        );
        if button.clicked() {
            egui::Popup::toggle_id(ui.ctx(), button.id);
        }
        egui::Popup::from_toggle_button_response(&button)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                ui.set_min_width(460.0);
                let search_id = egui::Id::new("openrouter_import_search");
                let mut search_text: String =
                    ui.data_mut(|d| d.get_temp(search_id).unwrap_or_default());
                let search = ui.add(
                    egui::TextEdit::singleline(&mut search_text)
                        .hint_text(text.preset_basics.search_placeholder)
                        .desired_width(440.0),
                );
                if search.changed() {
                    ui.data_mut(|d| d.insert_temp(search_id, search_text.clone()));
                }
                ui.separator();
                egui::ScrollArea::vertical()
                    .max_height(260.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let query = search_text.to_lowercase();
                        for model in models.iter().filter(|model| {
                            query.is_empty()
                                || model.name.to_lowercase().contains(&query)
                                || model.id.to_lowercase().contains(&query)
                        }) {
                            if render_openrouter_import_row(ui, config, text, model) {
                                *changed = true;
                                ui.data_mut(|d| d.remove_temp::<String>(search_id));
                                egui::Popup::toggle_id(ui.ctx(), button.id);
                            }
                        }
                    });
            });
    });
}

fn render_openrouter_import_row(
    ui: &mut egui::Ui,
    config: &mut Config,
    text: &LocaleText,
    model: &OpenRouterImportModel,
) -> bool {
    let mut added = false;
    ui.horizontal(|ui| {
        if ui.small_button("+").clicked() {
            add_imported_openrouter_model(config, model);
            added = true;
        }
        if ui
            .selectable_label(false, format!("{} - {}", model.name, model.id))
            .clicked()
        {
            add_imported_openrouter_model(config, model);
            added = true;
        }
        ui.label(custom_model_type_label(model.model_type, text));
    });
    added
}

pub(super) fn start_openrouter_import(api_key: String) {
    {
        let Ok(mut state) = OPENROUTER_IMPORT.lock() else {
            return;
        };
        if state.loading {
            return;
        }
        state.loading = true;
        state.error = None;
    }

    std::thread::spawn(move || {
        let result = fetch_openrouter_models(&api_key);
        if let Ok(mut state) = OPENROUTER_IMPORT.lock() {
            state.loading = false;
            match result {
                Ok(models) => {
                    state.models = models;
                    state.error = None;
                }
                Err(error) => {
                    state.error = Some(error);
                }
            }
        }
    });
}

fn fetch_openrouter_models(api_key: &str) -> Result<Vec<OpenRouterImportModel>, String> {
    let mut request = crate::api::client::UREQ_AGENT.get("https://openrouter.ai/api/v1/models");
    if !api_key.trim().is_empty() {
        request = request.header("Authorization", &format!("Bearer {}", api_key.trim()));
    }
    let resp = request
        .call()
        .map_err(|error| format!("OpenRouter scan failed: {error}"))?;
    let parsed: OpenRouterModelsResponse = resp
        .into_body()
        .read_json()
        .map_err(|error| format!("OpenRouter scan parse failed: {error}"))?;

    Ok(parse_openrouter_models(parsed.data))
}

/// Pure projection of a raw OpenRouter `data` array into import models. Kept free
/// of any HTTP/IO so the parse + type-inference seam is unit-testable.
fn parse_openrouter_models(data: Vec<serde_json::Value>) -> Vec<OpenRouterImportModel> {
    data.into_iter()
        .filter_map(|value| {
            let id = value.get("id")?.as_str()?.to_string();
            let name = value
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(&id)
                .to_string();
            Some(OpenRouterImportModel {
                model_type: infer_openrouter_model_type(&value, &id),
                id,
                name,
            })
        })
        .collect()
}

fn infer_openrouter_model_type(value: &serde_json::Value, id: &str) -> CustomModelType {
    let payload = value.to_string().to_lowercase();
    let id_lower = id.to_lowercase();
    if payload.contains("\"image\"")
        || id_lower.contains("vision")
        || id_lower.contains("-vl")
        || id_lower.contains("/vl")
        || id_lower.contains("llava")
    {
        CustomModelType::Vision
    } else {
        CustomModelType::Text
    }
}

fn add_imported_openrouter_model(config: &mut Config, model: &OpenRouterImportModel) {
    if config
        .custom_models
        .iter()
        .any(|existing| existing.full_name == model.id && existing.provider == "openrouter")
    {
        return;
    }
    let id = unique_custom_id("openrouter", &model.id, &config.custom_models);
    config.custom_models.push(CustomModelDefinition {
        id,
        provider: "openrouter".to_string(),
        display_name: model.name.clone(),
        full_name: model.id.clone(),
        model_type: model.model_type,
        enabled: true,
        quota_en: "OpenRouter quota".to_string(),
        quota_vi: "Theo OpenRouter".to_string(),
        quota_ko: "OpenRouter 기준".to_string(),
        supports_search: None,
    });
}

#[cfg(test)]
mod tests {
    use super::super::{slugify, unique_custom_id};
    use super::{infer_openrouter_model_type, parse_openrouter_models};
    use crate::config::types::{CustomModelDefinition, CustomModelType};
    use serde_json::json;

    fn model_def(id: &str) -> CustomModelDefinition {
        CustomModelDefinition {
            id: id.to_string(),
            provider: "openrouter".to_string(),
            display_name: id.to_string(),
            full_name: id.to_string(),
            model_type: CustomModelType::Text,
            enabled: true,
            quota_en: String::new(),
            quota_vi: String::new(),
            quota_ko: String::new(),
            supports_search: None,
        }
    }

    #[test]
    fn infers_vision_from_image_modality_payload() {
        let value = json!({
            "id": "some/plain-model",
            "architecture": { "input_modalities": ["text", "image"] }
        });
        assert_eq!(
            infer_openrouter_model_type(&value, "some/plain-model"),
            CustomModelType::Vision
        );
    }

    #[test]
    fn infers_vision_from_id_markers() {
        for id in [
            "qwen/qwen-vl",
            "x/model-vl",
            "anyscale/llava-13b",
            "a/vision-pro",
        ] {
            let value = json!({ "id": id });
            assert_eq!(
                infer_openrouter_model_type(&value, id),
                CustomModelType::Vision,
                "expected Vision for id {id}"
            );
        }
    }

    #[test]
    fn infers_text_when_no_vision_signal() {
        let value = json!({
            "id": "meta/llama-3-8b",
            "architecture": { "input_modalities": ["text"] }
        });
        assert_eq!(
            infer_openrouter_model_type(&value, "meta/llama-3-8b"),
            CustomModelType::Text
        );
    }

    #[test]
    fn parse_skips_entries_without_string_id_and_defaults_name() {
        let data = vec![
            json!({ "id": "a/text-model", "name": "Text Model" }),
            json!({ "name": "missing id" }),   // no id -> skipped
            json!({ "id": 42 }),               // non-string id -> skipped
            json!({ "id": "b/no-name" }),      // name falls back to id
            json!({ "id": "c/vision-model" }), // inferred Vision via id
        ];
        let models = parse_openrouter_models(data);
        assert_eq!(models.len(), 3);

        assert_eq!(models[0].id, "a/text-model");
        assert_eq!(models[0].name, "Text Model");
        assert_eq!(models[0].model_type, CustomModelType::Text);

        assert_eq!(models[1].id, "b/no-name");
        assert_eq!(models[1].name, "b/no-name"); // defaulted to id
        assert_eq!(models[1].model_type, CustomModelType::Text);

        assert_eq!(models[2].id, "c/vision-model");
        assert_eq!(models[2].model_type, CustomModelType::Vision);
    }

    #[test]
    fn slugify_lowercases_and_collapses_non_alnum_runs() {
        assert_eq!(slugify("Meta Llama 3"), "meta-llama-3");
        assert_eq!(slugify("openai/gpt-4o"), "openai-gpt-4o");
        assert_eq!(slugify("  --Trim__Edges--  "), "trim-edges");
        assert_eq!(slugify("Multiple   Spaces"), "multiple-spaces");
    }

    #[test]
    fn unique_custom_id_appends_suffix_on_collision_with_existing() {
        let base = "custom-openrouter-foo-bar";
        let existing = vec![model_def(base)];
        // First collision-free candidate is "<base>-2" (suffix starts at 2),
        // assuming the build-time catalog holds no matching ids.
        let id = unique_custom_id("openrouter", "foo/bar", &existing);
        assert_ne!(id, base, "must not reuse the colliding id");
        assert!(
            id == format!("{base}-2") || id.starts_with(&format!("{base}-")),
            "collision id should derive from base with a numeric suffix, got {id}"
        );
    }

    #[test]
    fn unique_custom_id_uses_base_when_no_collision() {
        let existing: Vec<CustomModelDefinition> = Vec::new();
        let id = unique_custom_id("openrouter", "brand/new-unique-xyz123", &existing);
        assert_eq!(id, "custom-openrouter-brand-new-unique-xyz123");
    }
}
