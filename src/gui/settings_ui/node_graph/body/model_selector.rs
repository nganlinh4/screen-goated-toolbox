// --- NODE BODY: MODEL SELECTOR & SETTINGS ---
// Shared model selector, prompt editor, and render mode UI for Special/Process nodes.

use std::collections::HashMap;

use super::super::utils::{
    insert_next_language_tag, model_supports_search, show_language_selector, show_language_vars,
};
use super::super::viewer::ChainViewer;
use crate::gui::icons::{Icon, icon_button};
use crate::gui::theme::AppTheme;
use crate::gui::widgets::filled_button;
use crate::model_config::{
    ModelType, get_all_models_with_ollama, get_model_by_id, is_ollama_scan_in_progress,
    model_is_non_llm, trigger_ollama_model_scan,
};
use eframe::egui;

const MODEL_BUTTON_WRAP_WIDTH: f32 = 128.0;
const PROMPT_EDITOR_WIDTH: f32 = 170.0;

/// Renders the model selector, prompt editor, language vars, and settings row
/// for Special and Process node bodies. Returns true if auto_copy was triggered.
#[expect(clippy::too_many_arguments)]
pub fn show_model_and_settings(
    ui: &mut egui::Ui,
    viewer: &mut ChainViewer,
    target_model_type: ModelType,
    model: &mut String,
    prompt: &mut String,
    language_vars: &mut HashMap<String, String>,
    show_overlay: &mut bool,
    streaming_enabled: &mut bool,
    render_mode: &mut String,
    auto_copy: &mut bool,
    auto_speak: &mut bool,
) -> bool {
    let mut auto_copy_triggered = false;

    // Row 1: Model
    let model_label = match viewer.ui_language.as_str() {
        "vi" => "Mô hình:",
        "ko" => "모델:",
        _ => "Model:",
    };
    ui.label(model_label);
    let model_def = get_model_by_id(model);
    let display_name = model_def
        .as_ref()
        .map(|m| m.localized_name(&viewer.ui_language).to_string())
        .unwrap_or_else(|| model.clone());

    ui.horizontal(|ui| {
        if let Some(m) = model_def.as_ref() {
            crate::gui::icons::draw_icon_static(
                ui,
                crate::gui::icons::provider_icon(&m.provider),
                Some(crate::gui::icons::ICON_MD),
            );
        }

        let button_response = ui
            .scope(|ui| {
                ui.set_max_width(MODEL_BUTTON_WRAP_WIDTH);
                ui.add(egui::Button::new(display_name).wrap())
            })
            .inner;
        if button_response.clicked() {
            egui::Popup::toggle_id(ui.ctx(), button_response.id);
            if viewer.use_ollama {
                trigger_ollama_model_scan();
            }
        }
        let popup_layer_id = button_response.id;
        egui::Popup::from_toggle_button_response(&button_response).show(|ui| {
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);

            if viewer.use_ollama && is_ollama_scan_in_progress() {
                let loading_text = match viewer.ui_language.as_str() {
                    "vi" => "⏳ Đang quét các model local...",
                    "ko" => "⏳ 로컬 모델 스캔 중...",
                    _ => "⏳ Scanning local models...",
                };
                ui.label(egui::RichText::new(loading_text).weak().italics());
                ui.separator();
            }

            for m in get_all_models_with_ollama() {
                if m.enabled
                    && m.model_type == target_model_type
                    && viewer.is_provider_enabled(&m.provider)
                {
                    let name = m.localized_name(&viewer.ui_language);
                    let quota = m.localized_quota(&viewer.ui_language);
                    let label = format!("{} - {} - {}", name, m.full_name, quota);
                    let is_selected = *model == m.id;

                    ui.horizontal(|ui| {
                        crate::gui::icons::draw_icon_static(
                            ui,
                            crate::gui::icons::provider_icon(&m.provider),
                            Some(crate::gui::icons::ICON_MD),
                        );
                        if ui.selectable_label(is_selected, label).clicked() {
                            *model = m.id.clone();
                            viewer.changed = true;
                            egui::Popup::toggle_id(ui.ctx(), popup_layer_id);
                        }
                        if model_supports_search(&m.id) {
                            crate::gui::icons::draw_icon_static(
                                ui,
                                Icon::Search,
                                Some(crate::gui::icons::ICON_XS),
                            );
                        }
                    });
                }
            }
        });
    });

    let uses_target_language_selector = get_model_by_id(model)
        .map(|m| {
            m.provider == "google-gtx"
                || (target_model_type == ModelType::Audio
                    && crate::model_config::is_gemini_live_translate_model_id(&m.id))
        })
        .unwrap_or(false);

    if uses_target_language_selector {
        let label = match viewer.ui_language.as_str() {
            "vi" => "Ngôn ngữ:",
            "ko" => "언어:",
            _ => "Language:",
        };
        show_language_selector(
            ui,
            label,
            1,
            "language1",
            language_vars,
            &mut viewer.changed,
        );
    }

    // Only show prompt UI for LLM models (not QR scanner, GTX, Whisper, etc.)
    if !model_is_non_llm(model) {
        // Row 2: Prompt Label + Add Tag Button
        ui.horizontal(|ui| {
            let prompt_label = match viewer.ui_language.as_str() {
                "vi" => "Lệnh:",
                "ko" => "프롬프트:",
                _ => "Prompt:",
            };
            ui.label(prompt_label);

            let btn_label = match viewer.ui_language.as_str() {
                "vi" => "+ Ngôn ngữ",
                "ko" => "+ 언어",
                _ => "+ Language",
            };
            let lang_btn_bg = AppTheme::from_ui(ui).node_button_fill();
            let clicked = ui
                .scope(|ui| {
                    ui.style_mut().override_text_style = Some(egui::TextStyle::Small);
                    filled_button(ui, btn_label, lang_btn_bg, egui::Color32::WHITE, 8)
                })
                .inner
                .clicked();
            if clicked {
                insert_next_language_tag(prompt, language_vars);
                viewer.changed = true;
            }
        });

        // Row 3: Prompt TextEdit
        if ui
            .add(
                egui::TextEdit::multiline(prompt)
                    .desired_width(PROMPT_EDITOR_WIDTH)
                    .desired_rows(2),
            )
            .changed()
        {
            viewer.changed = true;
        }

        // Row 4+: Language Variables
        show_language_vars(
            ui,
            &viewer.ui_language,
            prompt,
            language_vars,
            &mut viewer.changed,
            &mut viewer.language_search,
        );
    }

    // Bottom Row: Settings
    ui.horizontal(|ui| {
        let icon = if *show_overlay {
            Icon::EyeOpen
        } else {
            Icon::EyeClosed
        };
        if icon_button(ui, icon).clicked() {
            *show_overlay = !*show_overlay;
            viewer.changed = true;
        }

        if *show_overlay {
            show_render_mode_popup(ui, viewer, render_mode, streaming_enabled);
        }

        // Copy icon toggle
        {
            let copy_icon = if *auto_copy {
                Icon::Copy
            } else {
                Icon::CopyDisabled
            };
            if icon_button(ui, copy_icon)
                .on_hover_text(viewer.text.preset_editor.input_auto_copy_tooltip)
                .clicked()
            {
                *auto_copy = !*auto_copy;
                viewer.changed = true;
                if *auto_copy {
                    auto_copy_triggered = true;
                }
            }
        }

        // Speak icon toggle
        {
            let speak_icon = if *auto_speak {
                Icon::Speaker
            } else {
                Icon::SpeakerDisabled
            };
            if icon_button(ui, speak_icon)
                .on_hover_text(viewer.text.preset_editor.input_auto_speak_tooltip)
                .clicked()
            {
                *auto_speak = !*auto_speak;
                viewer.changed = true;
            }
        }
    });

    auto_copy_triggered
}

/// Render mode popup (Normal, Stream, Markdown, MD+Stream)
fn show_render_mode_popup(
    ui: &mut egui::Ui,
    viewer: &mut ChainViewer,
    render_mode: &mut String,
    streaming_enabled: &mut bool,
) {
    let current_mode_label = match (render_mode.as_str(), *streaming_enabled) {
        ("markdown_stream", _) => match viewer.ui_language.as_str() {
            "vi" => "Đẹp+Str",
            "ko" => "마크다운+스트림",
            _ => "MD+Stream",
        },
        ("markdown", _) => match viewer.ui_language.as_str() {
            "vi" => "Đẹp",
            "ko" => "마크다운",
            _ => "Markdown",
        },
        (_, true) => match viewer.ui_language.as_str() {
            "vi" => "Stream",
            "ko" => "스트림",
            _ => "Stream",
        },
        (_, false) => match viewer.ui_language.as_str() {
            "vi" => "Thường",
            "ko" => "일반",
            _ => "Normal",
        },
    };

    let btn_bg = AppTheme::from_ui(ui).node_button_fill();
    let btn = filled_button(ui, current_mode_label, btn_bg, egui::Color32::WHITE, 8);
    let popup_id = btn.id;
    if btn.clicked() {
        egui::Popup::toggle_id(ui.ctx(), popup_id);
    }
    egui::Popup::from_toggle_button_response(&btn)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui: &mut egui::Ui| {
            ui.set_min_width(80.0);
            let (lbl_norm, lbl_stm, lbl_md, lbl_md_stm) = match viewer.ui_language.as_str() {
                "vi" => ("Thường", "Stream", "Đẹp", "Đẹp+Str"),
                "ko" => ("일반", "스트림", "마크다운", "마크다운+스트림"),
                _ => ("Normal", "Stream", "Markdown", "MD+Stream"),
            };

            if ui
                .selectable_label(render_mode == "plain" && !*streaming_enabled, lbl_norm)
                .clicked()
            {
                *render_mode = "plain".to_string();
                *streaming_enabled = false;
                viewer.changed = true;
                egui::Popup::close_id(ui.ctx(), popup_id);
            }
            if ui
                .selectable_label(
                    (render_mode == "stream" || render_mode == "plain") && *streaming_enabled,
                    lbl_stm,
                )
                .clicked()
            {
                *render_mode = "stream".to_string();
                *streaming_enabled = true;
                viewer.changed = true;
                egui::Popup::close_id(ui.ctx(), popup_id);
            }
            if ui
                .selectable_label(render_mode == "markdown", lbl_md)
                .clicked()
            {
                *render_mode = "markdown".to_string();
                *streaming_enabled = false;
                viewer.changed = true;
                egui::Popup::close_id(ui.ctx(), popup_id);
            }
            if ui
                .selectable_label(render_mode == "markdown_stream", lbl_md_stm)
                .clicked()
            {
                *render_mode = "markdown_stream".to_string();
                *streaming_enabled = true;
                viewer.changed = true;
                egui::Popup::close_id(ui.ctx(), popup_id);
            }
        });
}
