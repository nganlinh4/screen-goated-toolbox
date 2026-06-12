use crate::config::Config;
use crate::config::types::{CustomModelDefinition, CustomModelType};
use crate::gui::icons::{self, Icon};
use crate::gui::locale::LocaleText;
use crate::gui::theme::{AppTheme, blend};
use crate::gui::widgets::filled_icon_button;
use crate::model_config::{
    ModelConfig, ModelSource, ModelType, get_all_models, get_all_models_with_custom,
    is_ollama_scan_in_progress, ollama_cached_model_count, trigger_ollama_model_scan,
};
use eframe::egui::{self, Color32, CornerRadius, Margin, Stroke};
use serde::Deserialize;
use std::sync::Mutex;

/// Representative accent for a provider — drives action buttons and row icons
/// so each provider reads at a glance.
///
/// Dark variants are lighter/brighter (legible on dark surfaces); light variants
/// are deeper/more saturated (legible as text on near-white surfaces). Used as a
/// solid color for icons/text/buttons; for fills, blend it into the surface with
/// [`wash`] so transparency never depends on what's painted underneath.
fn provider_accent(provider: &str, dark: bool) -> Color32 {
    let (d, l) = match provider {
        "google" => ((124, 156, 245), (66, 92, 210)),
        "groq" => ((236, 154, 74), (176, 92, 18)),
        "cerebras" => ((230, 116, 100), (192, 58, 42)),
        "openrouter" => ((112, 152, 236), (52, 96, 200)),
        "ollama" => ((96, 198, 152), (28, 140, 92)),
        _ => ((124, 154, 204), (64, 96, 168)),
    };
    let (r, g, b) = if dark { d } else { l };
    Color32::from_rgb(r, g, b)
}

/// Blend [color] into the card surface by fraction [t] to get a faint, *opaque*
/// tint. Unlike a low-alpha overlay, this reads identically regardless of what's
/// behind it, and adapts to dark/light because it starts from `card_bg`.
fn wash(theme: &AppTheme, color: Color32, t: f32) -> Color32 {
    blend(theme.card_bg(), color, t)
}

/// A legible on-color (near-black or white) for text/icons painted on top of a
/// solid [fill]. Picks by perceived luminance so it works for both the lighter
/// dark-mode accents and the deeper light-mode ones.
fn on_color(fill: Color32) -> Color32 {
    let luma = 0.299 * fill.r() as f32 + 0.587 * fill.g() as f32 + 0.114 * fill.b() as f32;
    if luma > 150.0 {
        Color32::from_rgb(20, 22, 28)
    } else {
        Color32::WHITE
    }
}

/// Paint a provider/status icon at [size], tinted [color], inline in the layout.
fn accent_icon(ui: &mut egui::Ui, icon: Icon, color: Color32, size: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    icons::paint_icon(ui.painter(), rect, icon, color);
}

#[derive(Clone, Debug)]
struct OpenRouterImportModel {
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

lazy_static::lazy_static! {
    static ref OPENROUTER_IMPORT: Mutex<OpenRouterImportState> =
        Mutex::new(OpenRouterImportState::default());
    static ref OLLAMA_SCAN_REQUESTED: Mutex<bool> = Mutex::new(false);
}

pub fn render_custom_models_modal(
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
    let modal = egui::Modal::new(egui::Id::new("custom_models_modal"))
        .backdrop_color(theme.scrim_color())
        .frame(theme.dialog_frame())
        .show(ui.ctx(), |ui| {
            ui.set_width(1120.0);
            ui.set_min_height(580.0);

            // Header: title + description on the left; the Import/Scan toolbar
            // buttons sit on the right, immediately left of the close (×) button.
            let dark = ui.visuals().dark_mode;
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(text.custom_models_title)
                        .size(18.0)
                        .strong()
                        .color(theme.on_surface()),
                );
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(text.custom_models_desc)
                        .size(11.5)
                        .color(theme.on_surface_variant()),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // right_to_left: first added is rightmost → close, then the
                    // two buttons render to its left as [Import][Scan][×].
                    if icons::icon_button(ui, Icon::Close).clicked() {
                        *show_modal = false;
                    }
                    ui.add_space(10.0);
                    let ollama_accent = provider_accent("ollama", dark);
                    if filled_icon_button(
                        ui,
                        Icon::Terminal,
                        text.custom_models_scan_ollama,
                        ollama_accent,
                        on_color(ollama_accent),
                        10,
                    )
                    .clicked()
                    {
                        if let Ok(mut requested) = OLLAMA_SCAN_REQUESTED.lock() {
                            *requested = true;
                        }
                        if config.use_ollama {
                            trigger_ollama_model_scan();
                        }
                    }
                    ui.add_space(8.0);
                    let openrouter_accent = provider_accent("openrouter", dark);
                    if filled_icon_button(
                        ui,
                        Icon::Public,
                        text.custom_models_import_openrouter,
                        openrouter_accent,
                        on_color(openrouter_accent),
                        10,
                    )
                    .clicked()
                    {
                        start_openrouter_import(config.openrouter_api_key.clone());
                    }
                });
            });
            ui.add_space(10.0);

            // Scan/import status line (results popup + ollama progress).
            ui.horizontal(|ui| {
                render_ollama_scan_status(ui, config);
            });
            render_openrouter_import_results(ui, config, text, &mut changed);
            ui.add_space(6.0);

            egui::ScrollArea::vertical()
                .max_height(520.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.columns(2, |columns| {
                        if render_provider_section(&mut columns[0], config, "google", text) {
                            changed = true;
                        }
                        columns[0].add_space(8.0);
                        for provider in ["groq", "cerebras", "openrouter", "ollama"] {
                            if render_provider_section(&mut columns[1], config, provider, text) {
                                changed = true;
                            }
                            columns[1].add_space(8.0);
                        }
                    });
                });
        });

    if modal.should_close() {
        *show_modal = false;
    }

    changed
}

fn render_provider_section(
    ui: &mut egui::Ui,
    config: &mut Config,
    provider: &str,
    text: &LocaleText,
) -> bool {
    let mut changed = false;
    let dark = ui.visuals().dark_mode;
    let theme = AppTheme::from_ui(ui);
    let accent = provider_accent(provider, dark);
    let all_models = get_all_models_with_custom(&config.custom_models);
    let provider_models: Vec<ModelConfig> = all_models
        .into_iter()
        .filter(|model| model.provider == provider)
        .collect();

    egui::Frame::new()
        .fill(theme.card_bg())
        .stroke(theme.card_stroke())
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());

            // Header: plain provider title with the add CTA pinned right.
            ui.horizontal(|ui| {
                accent_icon(
                    ui,
                    crate::gui::icons::provider_icon(provider),
                    theme.on_surface_variant(),
                    icons::ICON_MD,
                );
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(provider_label(provider))
                        .strong()
                        .size(14.0)
                        .color(theme.on_surface()),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if provider != "ollama"
                        && filled_icon_button(
                            ui,
                            Icon::Plus,
                            &add_label(provider, text),
                            accent,
                            on_color(accent),
                            8,
                        )
                        .clicked()
                    {
                        config
                            .custom_models
                            .push(new_custom_model(provider, &config.custom_models));
                        changed = true;
                    }
                });
            });
            ui.add_space(8.0);

            let builtins: Vec<&ModelConfig> = provider_models
                .iter()
                .filter(|model| model.source == ModelSource::BuiltIn)
                .collect();
            let discovered: Vec<&ModelConfig> = provider_models
                .iter()
                .filter(|model| model.source == ModelSource::Discovered)
                .collect();

            if !builtins.is_empty() {
                section_caption(ui, &theme, text.custom_models_builtin_locked);
                for model in &builtins {
                    render_locked_model_row(ui, &theme, model, text, &config.ui_language);
                }
                ui.add_space(6.0);
            }

            let mut delete_idx = None;
            for (idx, model) in config.custom_models.iter_mut().enumerate() {
                if model.provider != provider {
                    continue;
                }
                if render_user_model_row(ui, &theme, accent, model, text, &mut delete_idx, idx) {
                    changed = true;
                }
                ui.add_space(6.0);
            }
            if let Some(idx) = delete_idx {
                config.custom_models.remove(idx);
                changed = true;
            }

            if !discovered.is_empty() {
                section_caption(ui, &theme, text.custom_models_discovered_models);
                for model in discovered {
                    render_locked_model_row(ui, &theme, model, text, &config.ui_language);
                }
            } else if builtins.is_empty()
                && !config
                    .custom_models
                    .iter()
                    .any(|model| model.provider == provider)
            {
                ui.add_space(2.0);
                ui.label(
                    egui::RichText::new(text.custom_models_no_models)
                        .italics()
                        .color(theme.on_surface_variant()),
                );
            }
        });

    changed
}

/// Small uppercase-ish caption that separates locked / discovered groups.
fn section_caption(ui: &mut egui::Ui, theme: &AppTheme, label: &str) {
    ui.label(
        egui::RichText::new(label)
            .size(11.0)
            .color(theme.on_surface_variant()),
    );
    ui.add_space(3.0);
}

fn render_locked_model_row(
    ui: &mut egui::Ui,
    theme: &AppTheme,
    model: &ModelConfig,
    text: &LocaleText,
    ui_language: &str,
) {
    egui::Frame::new()
        .fill(wash(
            theme,
            theme.on_surface_variant(),
            if ui.visuals().dark_mode { 0.10 } else { 0.12 },
        ))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new(localized_model_name(model, ui_language))
                        .color(theme.on_surface()),
                );
                ui.label(
                    egui::RichText::new(&model.full_name)
                        .monospace()
                        .size(11.0)
                        .color(theme.on_surface_variant()),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    type_badge(ui, theme, model_type_label(model.model_type, text));
                });
            });
        });
    ui.add_space(4.0);
}

/// A pill badge for a model's modality (Text / Vision / Audio).
fn type_badge(ui: &mut egui::Ui, theme: &AppTheme, label: &str) {
    egui::Frame::new()
        .fill(wash(
            theme,
            theme.accent_fill(),
            if ui.visuals().dark_mode { 0.24 } else { 0.18 },
        ))
        .corner_radius(CornerRadius::same(7))
        .inner_margin(Margin::symmetric(7, 2))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(label)
                    .size(10.5)
                    .color(theme.on_surface()),
            );
        });
}

fn render_ollama_scan_status(ui: &mut egui::Ui, config: &Config) {
    if is_ollama_scan_in_progress() {
        ui.spinner();
        ui.label(ollama_status_text(&config.ui_language, "scanning", 0));
        return;
    }

    let requested = OLLAMA_SCAN_REQUESTED
        .lock()
        .map(|requested| *requested)
        .unwrap_or(false);
    if !requested {
        return;
    }

    if !config.use_ollama {
        ui.colored_label(
            ui.visuals().warn_fg_color,
            ollama_status_text(&config.ui_language, "disabled", 0),
        );
        return;
    }

    let count = ollama_cached_model_count();
    ui.label(ollama_status_text(&config.ui_language, "done", count));
}

fn render_user_model_row(
    ui: &mut egui::Ui,
    theme: &AppTheme,
    accent: Color32,
    model: &mut CustomModelDefinition,
    text: &LocaleText,
    delete_idx: &mut Option<usize>,
    idx: usize,
) -> bool {
    let mut changed = false;
    let dark = ui.visuals().dark_mode;
    egui::Frame::new()
        .fill(wash(theme, accent, if dark { 0.12 } else { 0.09 }))
        .stroke(Stroke::new(
            1.0,
            wash(theme, accent, if dark { 0.42 } else { 0.40 }),
        ))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(10))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                changed |= ui
                    .checkbox(&mut model.enabled, text.custom_models_enabled)
                    .changed();
                let mut supports_search = model.supports_search.unwrap_or(false);
                if ui
                    .checkbox(&mut supports_search, text.custom_models_search)
                    .changed()
                {
                    model.supports_search = Some(supports_search);
                    changed = true;
                }
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(text.custom_models_type).color(theme.on_surface_variant()),
                );
                crate::gui::widgets::combo(("custom_model_type", &model.id))
                    .selected_text(custom_model_type_label(model.model_type, text))
                    .show_ui(ui, |ui| {
                        changed |= ui
                            .selectable_value(
                                &mut model.model_type,
                                CustomModelType::Text,
                                text.custom_models_text_type,
                            )
                            .changed();
                        changed |= ui
                            .selectable_value(
                                &mut model.model_type,
                                CustomModelType::Vision,
                                text.custom_models_vision_type,
                            )
                            .changed();
                    });

                // Delete pinned to the far right as a danger-tinted icon button.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if delete_icon_button(ui, theme).clicked() {
                        *delete_idx = Some(idx);
                    }
                });
            });
            ui.add_space(6.0);
            labelled_field(
                ui,
                theme,
                text.custom_models_display_name,
                &mut model.display_name,
                &mut changed,
            );
            ui.add_space(4.0);
            labelled_field(
                ui,
                theme,
                text.custom_models_api_model,
                &mut model.full_name,
                &mut changed,
            );
        });
    changed
}

/// A left-aligned caption + a flex-filling single-line text field.
fn labelled_field(
    ui: &mut egui::Ui,
    theme: &AppTheme,
    label: &str,
    value: &mut String,
    changed: &mut bool,
) {
    ui.horizontal(|ui| {
        ui.add_sized(
            egui::vec2(96.0, ui.spacing().interact_size.y),
            egui::Label::new(egui::RichText::new(label).color(theme.on_surface_variant()))
                .selectable(false),
        );
        *changed |= ui
            .add(
                egui::TextEdit::singleline(value)
                    .desired_width(f32::INFINITY)
                    .margin(Margin::symmetric(8, 5)),
            )
            .changed();
    });
}

/// A compact danger-tinted trash button used to remove a custom model.
fn delete_icon_button(ui: &mut egui::Ui, theme: &AppTheme) -> egui::Response {
    let size = egui::vec2(28.0, 24.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let dark = ui.visuals().dark_mode;
    let fill = if response.is_pointer_button_down_on() {
        theme.danger_fill()
    } else if response.hovered() {
        wash(theme, theme.danger_fill(), 0.55)
    } else {
        wash(theme, theme.danger_fill(), if dark { 0.18 } else { 0.14 })
    };
    ui.painter().rect_filled(rect, CornerRadius::same(7), fill);
    let icon_color = if response.hovered() {
        theme.on_accent()
    } else {
        theme.danger_text()
    };
    let icon_rect =
        egui::Rect::from_center_size(rect.center(), egui::vec2(icons::ICON_MD, icons::ICON_MD));
    icons::paint_icon(ui.painter(), icon_rect, Icon::Delete, icon_color);
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}

fn render_openrouter_import_results(
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
            ui.label(text.custom_models_import_openrouter);
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
                        .hint_text(text.search_placeholder)
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

fn start_openrouter_import(api_key: String) {
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

    Ok(parsed
        .data
        .into_iter()
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
        .collect())
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

fn new_custom_model(provider: &str, existing: &[CustomModelDefinition]) -> CustomModelDefinition {
    let full_name = format!("{provider}/model");
    CustomModelDefinition {
        id: unique_custom_id(provider, &full_name, existing),
        provider: provider.to_string(),
        display_name: provider_label(provider).to_string(),
        full_name,
        model_type: CustomModelType::Text,
        enabled: true,
        quota_en: format!("{} quota", provider_label(provider)),
        quota_vi: format!("Theo {}", provider_label(provider)),
        quota_ko: format!("{} 기준", provider_label(provider)),
        supports_search: None,
    }
}

fn unique_custom_id(provider: &str, full_name: &str, existing: &[CustomModelDefinition]) -> String {
    let base = format!("custom-{}-{}", provider, slugify(full_name));
    let mut candidate = base.clone();
    let mut suffix = 2;
    while existing.iter().any(|model| model.id == candidate)
        || get_all_models().iter().any(|model| model.id == candidate)
    {
        candidate = format!("{base}-{suffix}");
        suffix += 1;
    }
    candidate
}

fn slugify(value: &str) -> String {
    let mut slug = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    slug.trim_matches('-').to_string()
}

fn add_label(provider: &str, text: &LocaleText) -> String {
    // The button already shows a leading Plus icon, so strip any "+ " the
    // localized label carries to avoid a doubled plus.
    if provider == "openrouter" {
        text.custom_models_add_openrouter
            .trim_start_matches('+')
            .trim_start()
            .to_string()
    } else {
        provider_label(provider).to_string()
    }
}

fn ollama_status_text(ui_language: &str, state: &str, count: usize) -> String {
    match (ui_language, state) {
        ("vi", "scanning") => "Đang quét Ollama...".to_string(),
        ("ko", "scanning") => "Ollama 검색 중...".to_string(),
        (_, "scanning") => "Scanning Ollama...".to_string(),
        ("vi", "disabled") => "Hãy bật Ollama ở phần Mã API trước.".to_string(),
        ("ko", "disabled") => "먼저 API 키 섹션에서 Ollama를 켜세요.".to_string(),
        (_, "disabled") => "Enable Ollama in API Keys first.".to_string(),
        ("vi", "done") => format!("Đã quét Ollama: {count} mô hình trong cache."),
        ("ko", "done") => format!("Ollama 검색 완료: 캐시된 모델 {count}개."),
        _ => format!("Ollama scan finished: {count} cached models."),
    }
}

fn localized_model_name<'a>(model: &'a ModelConfig, ui_language: &str) -> &'a str {
    match ui_language {
        "vi" => &model.name_vi,
        "ko" => &model.name_ko,
        _ => &model.name_en,
    }
}

fn provider_label(provider: &str) -> &str {
    match provider {
        "google" => "Gemini",
        "groq" => "Groq",
        "cerebras" => "Cerebras",
        "openrouter" => "OpenRouter",
        "ollama" => "Ollama",
        _ => provider,
    }
}

fn model_type_label(model_type: ModelType, text: &LocaleText) -> &'static str {
    match model_type {
        ModelType::Vision => text.custom_models_vision_type,
        ModelType::Text => text.custom_models_text_type,
        ModelType::Audio => text.node_input_audio,
    }
}

fn custom_model_type_label(model_type: CustomModelType, text: &LocaleText) -> &'static str {
    match model_type {
        CustomModelType::Text => text.custom_models_text_type,
        CustomModelType::Vision => text.custom_models_vision_type,
    }
}
