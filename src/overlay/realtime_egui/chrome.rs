use crate::APP;
use crate::api::realtime_audio::REALTIME_RMS;
use crate::overlay::realtime_webview::controller;
use crate::overlay::realtime_webview::state::*;
use eframe::egui;
use std::sync::atomic::Ordering;

use super::RealtimeUiState;
use super::style::{RealtimeEguiTheme, card_frame, compact_button, pill_frame, render_combo};

pub(super) fn render_transcription_header(
    ui: &mut egui::Ui,
    state: &mut RealtimeUiState,
    theme: &RealtimeEguiTheme,
    locale: &crate::gui::locale::LocaleText,
    is_device_mode: bool,
) {
    panel_header_frame(theme).show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
            render_volume_meter(ui, theme, 56.0);
            render_source_group(ui, theme, locale, is_device_mode);
            render_transcription_controls(ui, theme, locale);
            if compact_button(ui, "Copy", false, theme)
                .on_hover_text(locale.overlay_copy_tooltip)
                .clicked()
            {
                copy_visible_text(state.show_transcription, false);
            }
            render_font_group(ui, state, theme, locale);
            if compact_button(ui, "Hide", !state.show_transcription, theme)
                .on_hover_text(locale.toggle_transcription_tooltip)
                .clicked()
            {
                state.show_transcription = !state.show_transcription;
                if controller::set_visibility(state.show_transcription, state.show_translation) {
                    super::USER_REQUESTED_CLOSE.store(true, Ordering::SeqCst);
                }
            }
        });
    });
}

pub(super) fn render_translation_header(
    ui: &mut egui::Ui,
    state: &mut RealtimeUiState,
    theme: &RealtimeEguiTheme,
    locale: &crate::gui::locale::LocaleText,
    tts_enabled: bool,
) {
    panel_header_frame(theme).show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
            let tts_label = if tts_enabled { "TTS On" } else { "TTS" };
            if compact_button(ui, tts_label, tts_enabled, theme)
                .on_hover_text(locale.tts_settings_title)
                .clicked()
            {
                state.show_tts_panel = !state.show_tts_panel;
            }
            render_translation_model_menu(ui, theme, locale);
            render_language_selector(ui, theme);
            if compact_button(ui, "Copy", false, theme)
                .on_hover_text(locale.overlay_copy_tooltip)
                .clicked()
            {
                copy_visible_text(false, state.show_translation);
            }
            render_font_group(ui, state, theme, locale);
            if compact_button(ui, "Hide", !state.show_translation, theme)
                .on_hover_text(locale.toggle_translation_tooltip)
                .clicked()
            {
                state.show_translation = !state.show_translation;
                if controller::set_visibility(state.show_transcription, state.show_translation) {
                    super::USER_REQUESTED_CLOSE.store(true, Ordering::SeqCst);
                }
            }
        });
    });
}

fn panel_header_frame(theme: &RealtimeEguiTheme) -> egui::Frame {
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(6, 5))
        .corner_radius(egui::CornerRadius::same(8))
        .fill(theme.header)
        .stroke(egui::Stroke::new(1.0, theme.border.gamma_multiply(0.45)))
}

pub(super) fn render_device_warning(
    ui: &mut egui::Ui,
    theme: &RealtimeEguiTheme,
    locale: &crate::gui::locale::LocaleText,
) {
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(10, 5))
        .corner_radius(egui::CornerRadius::same(8))
        .fill(theme.warning.gamma_multiply(0.12))
        .stroke(egui::Stroke::new(1.0, theme.warning.gamma_multiply(0.45)))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(locale.device_mode_warning).color(theme.warning));
                if compact_button(ui, locale.select_app_btn, false, theme).clicked() {
                    crate::overlay::realtime_webview::app_selection::show_audio_app_selector_overlay();
                }
            });
        });
    ui.add_space(6.0);
}

pub(super) fn render_download_panel(
    ui: &mut egui::Ui,
    theme: &RealtimeEguiTheme,
    locale: &crate::gui::locale::LocaleText,
) {
    let download = REALTIME_STATE.lock().ok().and_then(|state| {
        if !state.is_downloading {
            return None;
        }
        Some((
            state.download_title.clone(),
            state.download_message.clone(),
            state.download_progress,
        ))
    });

    if let Some((title, message, progress)) = download {
        card_frame(theme).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(egui::RichText::new(title).strong().color(theme.text));
                ui.add(egui::ProgressBar::new(progress).desired_width(160.0));
                if !message.is_empty() {
                    ui.label(egui::RichText::new(message).color(theme.muted));
                }
                if compact_button(ui, locale.cancel_label, false, theme).clicked() {
                    controller::cancel_download();
                }
            });
        });
        ui.add_space(6.0);
    }
}

pub(super) fn render_tts_panel(
    ui: &mut egui::Ui,
    theme: &RealtimeEguiTheme,
    is_device_mode: bool,
    app_pid: u32,
    tts_enabled: bool,
    locale: &crate::gui::locale::LocaleText,
) {
    card_frame(theme).show(ui, |ui| {
        let mut tts_on = tts_enabled;
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(locale.tts_settings_title)
                    .strong()
                    .color(theme.text),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.checkbox(&mut tts_on, "TTS").changed() {
                    controller::set_tts_enabled(tts_on);
                }
            });
        });

        if is_device_mode && app_pid == 0 && tts_on {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(locale.device_mode_warning).color(theme.warning));
                if compact_button(ui, locale.select_app_btn, false, theme).clicked() {
                    crate::overlay::realtime_webview::app_selection::show_audio_app_selector_overlay();
                }
            });
        }

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(locale.realtime_tts_speed).color(theme.muted));

            let current_speed = CURRENT_TTS_SPEED.load(Ordering::Relaxed);
            let base_speed = REALTIME_TTS_SPEED.load(Ordering::Relaxed);
            let auto_speed = REALTIME_TTS_AUTO_SPEED.load(Ordering::Relaxed);

            let mut speed_val = base_speed as i32;
            if ui
                .add_sized(
                    [180.0, 20.0],
                    egui::Slider::new(&mut speed_val, 50..=200).show_value(false),
                )
                .changed()
            {
                controller::set_tts_speed(speed_val as u32);
            }

            ui.label(
                egui::RichText::new(format!("{:.1}x", current_speed as f32 / 100.0))
                    .color(theme.text),
            );

            let mut auto_on = auto_speed;
            if ui
                .checkbox(&mut auto_on, locale.realtime_tts_auto)
                .changed()
            {
                controller::set_tts_auto_speed(auto_on);
            }
        });

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(locale.realtime_tts_volume).color(theme.muted));

            let mut volume = CURRENT_TTS_VOLUME.load(Ordering::Relaxed) as i32;
            if ui
                .add_sized(
                    [180.0, 20.0],
                    egui::Slider::new(&mut volume, 0..=100).show_value(false),
                )
                .changed()
            {
                controller::set_tts_volume(volume as u32);
            }

            ui.label(egui::RichText::new(format!("{volume}%")).color(theme.text));
        });
    });
    ui.add_space(6.0);
}

fn render_volume_meter(ui: &mut egui::Ui, theme: &RealtimeEguiTheme, width: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 22.0), egui::Sense::hover());
    let rms = f32::from_bits(REALTIME_RMS.load(Ordering::Relaxed)).clamp(0.0, 1.0);
    let painter = ui.painter();
    let bar_count = 10;
    let bar_w = 4.0;
    let gap = 3.0;
    for i in 0..bar_count {
        let phase = (i as f32 / bar_count as f32) * std::f32::consts::TAU;
        let energy = (rms * 1.8 + phase.sin().abs() * 0.18).clamp(0.08, 1.0);
        let h = 3.0 + energy * 16.0;
        let x = rect.left() + i as f32 * (bar_w + gap);
        let y = rect.center().y - h / 2.0;
        let color = if i < 8 {
            theme.secondary
        } else {
            theme.primary
        };
        painter.rect_filled(
            egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(bar_w, h)),
            egui::CornerRadius::same(2),
            color.gamma_multiply(0.55 + energy * 0.45),
        );
    }
}

fn copy_visible_text(show_transcription: bool, show_translation: bool) {
    let text = REALTIME_STATE
        .lock()
        .map(|state| {
            let mut parts = Vec::new();
            if show_transcription {
                parts.push(state.full_transcript.clone());
            }
            if show_translation {
                let translation = format!(
                    "{}{}",
                    state.committed_translation, state.uncommitted_translation
                );
                parts.push(translation);
            }
            parts
                .into_iter()
                .filter(|part| !part.trim().is_empty())
                .collect::<Vec<_>>()
                .join("\n\n")
        })
        .unwrap_or_default();

    if !text.trim().is_empty() {
        crate::overlay::utils::copy_to_clipboard(
            &text,
            windows::Win32::Foundation::HWND::default(),
        );
    }
}

fn render_source_group(
    ui: &mut egui::Ui,
    theme: &RealtimeEguiTheme,
    locale: &crate::gui::locale::LocaleText,
    is_device_mode: bool,
) {
    pill_frame(theme).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 1.0;
            if compact_button(ui, "Mic", !is_device_mode, theme)
                .on_hover_text(locale.audio_src_mic)
                .clicked()
            {
                controller::set_audio_source("mic");
            }
            if compact_button(ui, "Device", is_device_mode, theme)
                .on_hover_text(locale.audio_src_device)
                .clicked()
            {
                controller::set_audio_source("device");
            }
        });
    });
}

fn render_font_group(
    ui: &mut egui::Ui,
    state: &mut RealtimeUiState,
    theme: &RealtimeEguiTheme,
    locale: &crate::gui::locale::LocaleText,
) {
    pill_frame(theme).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 1.0;
            if compact_button(ui, "-", false, theme)
                .on_hover_text(locale.font_minus_tooltip)
                .clicked()
            {
                state.font_size = (state.font_size - 2.0).max(10.0);
                controller::set_font_size(state.font_size as u32);
            }
            if compact_button(ui, "+", false, theme)
                .on_hover_text(locale.font_plus_tooltip)
                .clicked()
            {
                state.font_size = (state.font_size + 2.0).min(40.0);
                controller::set_font_size(state.font_size as u32);
            }
        });
    });
}

fn render_transcription_controls(
    ui: &mut egui::Ui,
    theme: &RealtimeEguiTheme,
    locale: &crate::gui::locale::LocaleText,
) {
    let (current_model, current_language) = APP
        .lock()
        .map(|a| {
            (
                crate::model_config::normalize_realtime_transcription_model_id(
                    &a.config.realtime_transcription_model,
                ),
                controller::normalize_transcription_language(
                    &a.config.realtime_transcription_language,
                ),
            )
        })
        .unwrap_or_else(|_| {
            (
                crate::model_config::GEMINI_LIVE_AUDIO_MODEL_ID_2_5.to_string(),
                "en".to_string(),
            )
        });

    render_combo(
        ui,
        "realtime_egui_transcription_model",
        transcription_model_label(&current_model),
        96.0,
        theme,
        |ui| {
            let models = [
                (
                    crate::model_config::GEMINI_LIVE_AUDIO_MODEL_ID_2_5,
                    "Gemini Live",
                ),
                ("gemini-live-s2s", "Gemini S2S"),
                ("parakeet", "Parakeet"),
                (crate::model_config::QWEN3_ASR_0_6B_MODEL_ID, "Qwen3 0.6B"),
                (crate::model_config::QWEN3_ASR_1_7B_MODEL_ID, "Qwen3 1.7B"),
                ("zipformer", "Zipformer"),
            ];
            for (id, label) in models {
                if ui.selectable_label(current_model == id, label).clicked() {
                    controller::set_transcription_model(id);
                    ui.close();
                }
            }
        },
    )
    .on_hover_text(locale.realtime_tooltip_transcription_model);

    ui.add_enabled_ui(current_model == "zipformer", |ui| {
        render_combo(
            ui,
            "realtime_egui_transcription_language",
            current_language.to_uppercase(),
            58.0,
            theme,
            |ui| {
                let languages = [
                    ("en", "English"),
                    ("ko", "Korean"),
                    ("zh", "Chinese"),
                    ("fr", "French"),
                    ("de", "German"),
                    ("es", "Spanish"),
                    ("ru", "Russian"),
                    ("all-8", "AR,EN,ID,JA,RU,TH,VI,ZH"),
                ];
                for (code, label) in languages {
                    if ui
                        .selectable_label(current_language == code, label)
                        .clicked()
                    {
                        controller::set_transcription_language(code);
                        ui.close();
                    }
                }
            },
        )
        .on_hover_text(locale.realtime_tooltip_transcription_language);
    });
}

fn transcription_model_label(model: &str) -> &'static str {
    match model {
        "parakeet" => "Parakeet",
        "zipformer" => "Zipformer",
        "gemini-live-s2s" => "Gemini S2S",
        id if id == crate::model_config::QWEN3_ASR_0_6B_MODEL_ID => "Qwen 0.6B",
        id if id == crate::model_config::QWEN3_ASR_1_7B_MODEL_ID => "Qwen 1.7B",
        _ => "Gemini",
    }
}

fn render_translation_model_menu(
    ui: &mut egui::Ui,
    theme: &RealtimeEguiTheme,
    locale: &crate::gui::locale::LocaleText,
) {
    let current_model = APP
        .lock()
        .map(|a| a.config.realtime_translation_model.clone())
        .unwrap_or_default();
    let model_label = match current_model.as_str() {
        "google-gemma" => "Gemma",
        "google-gtx" => locale.google_gtx_label,
        _ => "Cerebras",
    };

    render_combo(
        ui,
        "realtime_egui_translation_model",
        model_label,
        82.0,
        theme,
        |ui| {
            if ui
                .selectable_label(
                    current_model == crate::model_config::REALTIME_TRANSLATION_MODEL_CEREBRAS,
                    "Cerebras",
                )
                .clicked()
            {
                controller::set_translation_model(
                    crate::model_config::REALTIME_TRANSLATION_MODEL_CEREBRAS,
                );
                ui.close();
            }
            if ui
                .selectable_label(current_model == "google-gemma", "Gemma")
                .clicked()
            {
                controller::set_translation_model("google-gemma");
                ui.close();
            }
            if ui
                .selectable_label(
                    current_model == "google-gtx",
                    locale.google_gtx_label.to_string(),
                )
                .clicked()
            {
                controller::set_translation_model("google-gtx");
                ui.close();
            }
        },
    );
}

fn render_language_selector(ui: &mut egui::Ui, theme: &RealtimeEguiTheme) {
    let current_lang = NEW_TARGET_LANGUAGE
        .lock()
        .map(|l| {
            if l.is_empty() {
                "English".to_string()
            } else {
                l.clone()
            }
        })
        .unwrap_or_else(|_| "English".to_string());
    let lang_code = isolang::Language::from_name(&current_lang)
        .and_then(|l| l.to_639_1())
        .map(|c| c.to_uppercase())
        .unwrap_or_else(|| {
            current_lang
                .chars()
                .take(2)
                .collect::<String>()
                .to_uppercase()
        });

    let btn_resp = compact_button(ui, &lang_code, false, theme);
    if btn_resp.clicked() {
        egui::Popup::toggle_id(ui.ctx(), btn_resp.id);
    }
    let popup_id = btn_resp.id;

    egui::Popup::from_toggle_button_response(&btn_resp)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            ui.set_min_width(120.0);
            let search_id = egui::Id::new("realtime_lang_search");
            let mut search_text: String =
                ui.data_mut(|d| d.get_temp(search_id).unwrap_or_default());

            let response = ui.add(
                egui::TextEdit::singleline(&mut search_text)
                    .hint_text("Search...")
                    .desired_width(120.0),
            );
            if response.changed() {
                ui.data_mut(|d| d.insert_temp(search_id, search_text.clone()));
            }
            if response.clicked() {
                response.request_focus();
            }

            ui.separator();

            egui::ScrollArea::vertical()
                .max_height(250.0)
                .show(ui, |ui| {
                    for lang in crate::config::get_all_languages() {
                        let matches = search_text.is_empty()
                            || lang.to_lowercase().contains(&search_text.to_lowercase());
                        if matches && ui.selectable_label(current_lang == *lang, lang).clicked() {
                            controller::set_target_language(lang);
                            ui.data_mut(|d| d.remove_temp::<String>(search_id));
                            egui::Popup::toggle_id(ui.ctx(), popup_id);
                        }
                    }
                });
        });
}
