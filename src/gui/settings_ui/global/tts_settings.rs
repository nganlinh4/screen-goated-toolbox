use crate::config::tts_catalog::{
    GEMINI_VOICES, SUPERTONIC_LANGUAGE_SUMMARY, SUPPORTED_GEMINI_INSTRUCTION_LANGUAGES,
};
use crate::config::{Config, TtsMethod};
use crate::gui::icons::{Icon, draw_icon_static, icon_button};
use crate::gui::locale::LocaleText;
use crate::gui::theme::AppTheme;
use eframe::egui;

mod preview;
mod providers;

use preview::speak_settings_preview;
use providers::{
    render_kokoro_settings, render_magpie_settings, render_step_audio_settings,
    render_supertonic_settings, render_vieneu_settings,
};

pub fn render_tts_settings_modal(
    ui: &mut egui::Ui,
    config: &mut Config,
    text: &LocaleText,
    show_modal: &mut bool,
) -> bool {
    if !*show_modal {
        return false;
    }

    let mut changed = false;

    // Canonical voice list (Name, Gender) lives in the shared TTS catalog.
    let male_voices: Vec<_> = GEMINI_VOICES.iter().filter(|(_, g)| *g == "Male").collect();
    let female_voices: Vec<_> = GEMINI_VOICES
        .iter()
        .filter(|(_, g)| *g == "Female")
        .collect();

    let ctx = ui.ctx().clone();
    let theme = AppTheme::from_dark(ctx.global_style().visuals.dark_mode);

    // Manual full-viewport scrim behind the (tall, fixed-size) settings window
    // so it reads as the clear focus, matching the modal dialog treatment.
    let screen_rect = ctx.content_rect();
    ctx.layer_painter(egui::LayerId::new(
        egui::Order::Background,
        egui::Id::new("tts_settings_scrim"),
    ))
    .rect_filled(screen_rect, 0.0, theme.scrim_color());

    egui::Window::new(text.tts_playground.tts_settings_title)
        .collapsible(false)
        .resizable(false)
        .title_bar(false)
        .frame(theme.dialog_frame())
        // Match `set_max_width` below so the window isn't wider than its capped
        // content — otherwise the right-aligned header close (×) sits ~40px short
        // of the window edge.
        .default_width(820.0)
        .default_height(600.0)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(&ctx, |ui| {
            ui.set_min_height(500.0); // Force minimum height for the content area
            // Cap the content width so long description labels wrap and the
            // auto-sizing Window can't grow past the screen edges. Kept in sync
            // with `default_width` above so the header × reaches the right edge.
            ui.set_max_width(820.0);

            // Split the bundled "Title (feature scope)" locale string so the
            // parenthetical feature-scope hint renders as the muted dialog
            // description beneath the title instead of inline in the title.
            let (title_text, scope_hint) = match text.tts_playground.tts_settings_title.split_once('(') {
                Some((title, hint)) => (
                    title.trim_end(),
                    Some(hint.trim_end_matches(')').trim()),
                ),
                None => (text.tts_playground.tts_settings_title, None),
            };
            let mut close_dialog = false;
            ui.horizontal(|ui| {
                draw_icon_static(ui, Icon::Speaker, Some(crate::gui::icons::ICON_LG));
                close_dialog = crate::gui::widgets::dialog_header(
                    ui,
                    &theme,
                    title_text,
                    scope_hint,
                    |_| {},
                );
            });
            if close_dialog {
                *show_modal = false;
            }

            if config.tts_method == TtsMethod::VoxtralTts {
                config.tts_method = TtsMethod::VieneuTts;
                changed = true;
            }

            // === TTS METHOD SELECTION === (dropdown — too many options for a row)
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(text.tts_settings.tts_method_label).strong());
                let current_label = match config.tts_method {
                    TtsMethod::GeminiLive => text.tts_settings.tts_method_standard,
                    TtsMethod::EdgeTTS => text.tts_settings.tts_method_edge,
                    TtsMethod::GoogleTranslate => text.tts_settings.tts_method_fast,
                    TtsMethod::StepAudioEditX => "Step Audio EditX",
                    TtsMethod::MagpieMultilingual => "NVIDIA Magpie-Multilingual 357M",
                    TtsMethod::Kokoro => "Kokoro 82M v1.0",
                    TtsMethod::Supertonic => "Supertonic 3",
                    TtsMethod::VieneuTts | TtsMethod::VoxtralTts => "VieNeu-TTS v2",
                    // Deprecated/hidden (migrated away on load) — never a real option.
                    TtsMethod::FishAudioS2Pro => text.tts_settings.tts_method_standard,
                };
                crate::gui::widgets::combo("tts_method_combo")
                    .selected_text(current_label)
                    .width(300.0)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_value(
                                &mut config.tts_method,
                                TtsMethod::GeminiLive,
                                text.tts_settings.tts_method_standard,
                            )
                            .clicked()
                        {
                            changed = true;
                        }
                        if ui
                            .selectable_value(
                                &mut config.tts_method,
                                TtsMethod::EdgeTTS,
                                text.tts_settings.tts_method_edge,
                            )
                            .clicked()
                        {
                            changed = true;
                        }
                        if ui
                            .selectable_value(
                                &mut config.tts_method,
                                TtsMethod::GoogleTranslate,
                                text.tts_settings.tts_method_fast,
                            )
                            .clicked()
                        {
                            if config.tts_speed == "Fast" {
                                config.tts_speed = "Normal".to_string();
                            }
                            changed = true;
                        }
                        if ui
                            .selectable_value(
                                &mut config.tts_method,
                                TtsMethod::StepAudioEditX,
                                "Step Audio EditX",
                            )
                            .on_hover_text("Supports Mandarin, English, Sichuanese, Cantonese, Japanese, and Korean.")
                            .clicked()
                        {
                            changed = true;
                        }
                        if ui
                            .selectable_value(
                                &mut config.tts_method,
                                TtsMethod::MagpieMultilingual,
                                "NVIDIA Magpie-Multilingual 357M",
                            )
                            .on_hover_text(text.auxiliary.managed_tools.tool_desc_magpie)
                            .clicked()
                        {
                            changed = true;
                        }
                        if ui
                            .selectable_value(
                                &mut config.tts_method,
                                TtsMethod::Kokoro,
                                "Kokoro 82M v1.0",
                            )
                            .on_hover_text("Supports English, Mandarin Chinese, Japanese, Spanish, French, Hindi, Italian, and Portuguese.")
                            .clicked()
                        {
                            changed = true;
                        }
                        if ui
                            .selectable_value(
                                &mut config.tts_method,
                                TtsMethod::Supertonic,
                                "Supertonic 3",
                            )
                            .on_hover_text(SUPERTONIC_LANGUAGE_SUMMARY)
                            .clicked()
                        {
                            changed = true;
                        }
                        if ui
                            .selectable_value(
                                &mut config.tts_method,
                                TtsMethod::VieneuTts,
                                "VieNeu-TTS v2",
                            )
                            .on_hover_text("Vietnamese-first local TTS with English/Vietnamese code-switching and zero-shot voice cloning.")
                            .clicked()
                        {
                            changed = true;
                        }
                    });
            });
            ui.add_space(10.0);
            ui.separator();
            ui.add_space(10.0);

            // Speed and Tone & Style side by side
            if config.tts_method == TtsMethod::GeminiLive {
                ui.label(egui::RichText::new(text.tts_settings.tts_gemini_model_label).strong());
                ui.horizontal(|ui| {
                    for (api_model, label) in crate::model_config::tts_gemini_model_options() {
                        if ui
                            .radio_value(
                                &mut config.tts_gemini_live_model,
                                (*api_model).to_string(),
                                *label,
                            )
                            .clicked()
                        {
                            changed = true;
                        }
                    }
                });
                ui.add_space(10.0);

                ui.columns(2, |columns| {
                    // Left column: Speed
                    columns[0].label(egui::RichText::new(text.tts_settings.tts_speed_label).strong());
                    columns[0].horizontal(|ui| {
                        if ui.radio_value(&mut config.tts_speed, "Slow".to_string(), text.tts_settings.tts_speed_slow).clicked() { changed = true; }
                        if ui.radio_value(&mut config.tts_speed, "Normal".to_string(), text.tts_settings.tts_speed_normal).clicked() { changed = true; }
                        if ui.radio_value(&mut config.tts_speed, "Fast".to_string(), text.tts_settings.tts_speed_fast).clicked() { changed = true; }
                    });

                    // Right column: Language-Specific Instructions
                    columns[1].label(egui::RichText::new(text.tts_settings.tts_instructions_label).strong());

                    // Supported languages (ISO 639-3 → display name) live in the
                    // shared TTS catalog.
                    let supported_languages = SUPPORTED_GEMINI_INSTRUCTION_LANGUAGES;

                    // Show existing conditions
                    let mut to_remove: Option<usize> = None;
                    for (idx, condition) in config.tts_language_conditions.iter_mut().enumerate() {
                        columns[1].horizontal(|ui| {
                            // Language dropdown (read-only display for now)
                            let display_name = supported_languages.iter()
                                .find(|(code, _)| code.eq_ignore_ascii_case(&condition.language_code))
                                .map(|(_, name)| *name)
                                .unwrap_or(&condition.language_name);

                            ui.label(egui::RichText::new(display_name).strong().color(theme.success()));
                            crate::gui::icons::draw_icon_static(
                                ui,
                                crate::gui::icons::Icon::ArrowRightAlt,
                                Some(crate::gui::icons::ICON_SM),
                            );

                            // Instruction input
                            if ui.add(
                                egui::TextEdit::singleline(&mut condition.instruction)
                                    .desired_width(180.0)
                                    .hint_text(text.tts_settings.tts_instructions_hint)
                            ).changed() {
                                changed = true;
                            }

                            // Remove button - use Icon::Close for proper rendering
                            if icon_button(ui, Icon::Close).on_hover_text(text.tts_advanced.remove_label).clicked() {
                                to_remove = Some(idx);
                            }
                        });
                    }

                    // Remove condition if needed
                    if let Some(idx) = to_remove {
                        config.tts_language_conditions.remove(idx);
                        changed = true;
                    }

                    // Add condition dropdown - selecting immediately adds the condition
                    columns[1].horizontal(|ui| {
                        // Get languages that are not yet used
                        let used_codes: Vec<_> = config.tts_language_conditions.iter()
                            .map(|c| c.language_code.as_str())
                            .collect();
                        let available: Vec<_> = supported_languages.iter()
                            .filter(|(code, _)| !used_codes.contains(code))
                            .collect();

                        if !available.is_empty() {
                            // Dropdown that immediately adds selected language
                            crate::gui::widgets::combo("tts_add_condition")
                                .selected_text(text.tts_settings.tts_add_condition)
                                .width(140.0)
                                .show_ui(ui, |ui| {
                                    for (code, name) in &available {
                                        if ui.selectable_label(false, *name).clicked() {
                                            config.tts_language_conditions.push(crate::config::TtsLanguageCondition {
                                                language_code: code.to_string(),
                                                language_name: name.to_string(),
                                                instruction: String::new(),
                                            });
                                            changed = true;
                                        }
                                    }
                                });
                        }
                    });
                });

                ui.add_space(10.0);
                ui.separator();
                ui.add_space(10.0);

                // Voice selection - 4 columns layout to save vertical space
                ui.columns(4, |columns| {
                    // Helper to render a voice item
                    let render_voice = |ui: &mut egui::Ui, name: &str, config: &mut Config, text: &LocaleText, changed: &mut bool| {
                        ui.horizontal(|ui| {
                            let is_selected = config.tts_voice == name;
                            if ui.radio(is_selected, "").clicked() {
                                config.tts_voice = name.to_string();
                                *changed = true;
                            }
                            if icon_button(ui, Icon::Speaker)
                                .on_hover_text(text.tts_settings.tts_preview_label)
                                .clicked()
                            {
                                config.tts_voice = name.to_string();
                                *changed = true;
                                speak_settings_preview(text, name);
                            }
                            ui.label(egui::RichText::new(name).strong());
                        });
                    };

                    // Split male voices into 2 columns
                    let male_mid = male_voices.len().div_ceil(2);
                    let male_col1: Vec<_> = male_voices.iter().take(male_mid).collect();
                    let male_col2: Vec<_> = male_voices.iter().skip(male_mid).collect();

                    // Split female voices into 2 columns
                    let female_mid = female_voices.len().div_ceil(2);
                    let female_col1: Vec<_> = female_voices.iter().take(female_mid).collect();
                    let female_col2: Vec<_> = female_voices.iter().skip(female_mid).collect();

                    // Column 0: Male (first half)
                    columns[0].vertical(|ui| {
                        ui.label(egui::RichText::new(text.tts_settings.tts_male).strong().underline());
                        ui.add_space(4.0);
                        for (name, _) in male_col1 {
                            render_voice(ui, name, config, text, &mut changed);
                        }
                    });

                    // Column 1: Male (second half)
                    columns[1].vertical(|ui| {
                        ui.label(egui::RichText::new("").strong()); // Empty header for alignment
                        ui.add_space(4.0);
                        for (name, _) in male_col2 {
                            render_voice(ui, name, config, text, &mut changed);
                        }
                    });

                    // Column 2: Female (first half)
                    columns[2].vertical(|ui| {
                        ui.label(egui::RichText::new(text.tts_settings.tts_female).strong().underline());
                        ui.add_space(4.0);
                        for (name, _) in female_col1 {
                            render_voice(ui, name, config, text, &mut changed);
                        }
                    });

                    // Column 3: Female (second half)
                    columns[3].vertical(|ui| {
                        ui.label(egui::RichText::new("").strong()); // Empty header for alignment
                        ui.add_space(4.0);
                        for (name, _) in female_col2 {
                            render_voice(ui, name, config, text, &mut changed);
                        }
                    });
                });
            } else if config.tts_method == TtsMethod::GoogleTranslate {
                // Simplified UI for Google Translate
                ui.vertical_centered(|ui| {
                    ui.add_space(20.0);
                    ui.label(egui::RichText::new(text.tts_settings.tts_google_translate_title).size(18.0).strong());
                    ui.add_space(10.0);
                    ui.label(text.tts_settings.tts_google_translate_desc);
                    ui.add_space(20.0);

                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(text.tts_settings.tts_speed_label).strong());
                        if ui.radio_value(&mut config.tts_speed, "Slow".to_string(), text.tts_settings.tts_speed_slow).clicked() { changed = true; }
                        if ui.radio_value(&mut config.tts_speed, "Normal".to_string(), text.tts_settings.tts_speed_normal).clicked() { changed = true; }
                    });

                    ui.add_space(12.0);
                    if icon_button(ui, Icon::Speaker)
                        .on_hover_text(text.tts_settings.tts_preview_label)
                        .clicked()
                    {
                        speak_settings_preview(text, "Google Translate");
                    }

                    ui.add_space(20.0);
                });
            } else if config.tts_method == TtsMethod::EdgeTTS {
                // Trigger voice list loading on first render
                crate::api::tts::edge_voices::load_edge_voices_async();

                // Edge TTS Settings
                ui.vertical_centered(|ui| {
                    ui.add_space(10.0);
                    ui.label(egui::RichText::new(text.tts_settings.tts_edge_title).size(18.0).strong());
                    ui.add_space(5.0);
                    ui.label(text.tts_settings.tts_edge_desc);
                    ui.add_space(15.0);
                });

                // Pitch and Rate sliders
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(text.tts_settings.tts_pitch_label).strong());
                    if ui.add(egui::Slider::new(&mut config.edge_tts_settings.pitch, -50..=50).suffix(" Hz")).changed() {
                        changed = true;
                    }
                });

                ui.add_space(5.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(text.tts_settings.tts_rate_label).strong());
                    if ui.add(egui::Slider::new(&mut config.edge_tts_settings.rate, -50..=100).suffix("%")).changed() {
                        changed = true;
                    }
                });

                ui.add_space(15.0);
                ui.separator();
                ui.add_space(10.0);

                // Per-language voice configuration
                ui.label(egui::RichText::new(text.tts_settings.tts_voice_per_language_label).strong());
                ui.add_space(5.0);

                // Check voice cache status
                let cache_status = {
                    let cache = crate::api::tts::edge_voices::EDGE_VOICE_CACHE.lock().unwrap();
                    (cache.loaded, cache.loading, cache.error.clone())
                };

                if cache_status.1 {
                    // Loading
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(text.tts_settings.tts_loading_voices);
                    });
                } else if let Some(ref error) = cache_status.2 {
                    // Error
                    ui.colored_label(theme.danger_text(), format!("{} {}", text.tts_settings.tts_failed_load_voices, error).replace("{}", ""));
                    if ui.button(text.tts_settings.tts_retry_label).clicked() {
                        // Reset cache and retry
                        let mut cache = crate::api::tts::edge_voices::EDGE_VOICE_CACHE.lock().unwrap();
                        cache.loaded = false;
                        cache.loading = false;
                        cache.error = None;
                    }
                } else if cache_status.0 {
                    // Loaded - show voice configuration
                    egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
                        let mut to_remove: Option<usize> = None;

                        for (idx, voice_config) in config.edge_tts_settings.voice_configs.iter_mut().enumerate() {
                            ui.horizontal(|ui| {
                                // Language name (read-only)
                                ui.label(egui::RichText::new(&voice_config.language_name).strong().color(theme.success()));
                                crate::gui::icons::draw_icon_static(
                                ui,
                                crate::gui::icons::Icon::ArrowRightAlt,
                                Some(crate::gui::icons::ICON_SM),
                            );

                                // Voice dropdown for this language
                                let voices = crate::api::tts::edge_voices::get_voices_for_language(&voice_config.language_code);

                                crate::gui::widgets::combo(format!("edge_voice_{}", idx))
                                    .selected_text(&voice_config.voice_name)
                                    .width(220.0)
                                    .show_ui(ui, |ui| {
                                        for voice in &voices {
                                            let display = format!("{} ({})", voice.short_name, voice.gender);
                                            if ui.selectable_label(voice_config.voice_name == voice.short_name, &display).clicked() {
                                                voice_config.voice_name = voice.short_name.clone();
                                                changed = true;
                                            }
                                        }
                                    });

                                if icon_button(ui, Icon::Speaker)
                                    .on_hover_text(text.tts_settings.tts_preview_label)
                                    .clicked()
                                {
                                    speak_settings_preview(text, &voice_config.voice_name);
                                }

                                // Remove button
                                if icon_button(ui, Icon::Close).on_hover_text(text.tts_advanced.remove_label).clicked() {
                                    to_remove = Some(idx);
                                }
                            });
                        }

                        if let Some(idx) = to_remove {
                            config.edge_tts_settings.voice_configs.remove(idx);
                            changed = true;
                        }
                    });

                    ui.add_space(10.0);

                    // Add language dropdown
                    ui.horizontal(|ui| {
                        let used_codes: Vec<_> = config.edge_tts_settings.voice_configs.iter()
                            .map(|c| c.language_code.as_str())
                            .collect();

                        let available_langs = crate::api::tts::edge_voices::get_available_languages();
                        let available: Vec<_> = available_langs.iter()
                            .filter(|(code, _)| !used_codes.contains(&code.as_str()))
                            .collect();

                        if !available.is_empty() {
                            crate::gui::widgets::combo("edge_add_language")
                                .selected_text(text.tts_settings.tts_add_language_label)
                                .width(150.0)
                                .show_ui(ui, |ui| {
                                    for (code, name) in &available {
                                        if ui.selectable_label(false, name).clicked() {
                                            // Get first voice for this language as default
                                            let voices = crate::api::tts::edge_voices::get_voices_for_language(code);
                                            let default_voice = voices.first()
                                                .map(|v| v.short_name.clone())
                                                .unwrap_or_else(|| format!("{}-??-??Neural", code));

                                            config.edge_tts_settings.voice_configs.push(
                                                crate::config::EdgeTtsVoiceConfig {
                                                    language_code: code.clone(),
                                                    language_name: name.clone(),
                                                    voice_name: default_voice,
                                                }
                                            );
                                            changed = true;
                                        }
                                    }
                                });
                        }

                        if ui.button(text.tts_settings.tts_reset_to_defaults_label).clicked() {
                            config.edge_tts_settings = crate::config::EdgeTtsSettings::default();
                            config.tts_playground = crate::config::TtsPlaygroundSettings::default();
                            changed = true;
                        }
                    });
                } else {
                    // Not loaded yet, show loading message
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(text.tts_settings.tts_initializing_voices);
                    });
                }
            } else if config.tts_method == TtsMethod::StepAudioEditX {
                changed |= render_step_audio_settings(ui, config, text);
            } else if config.tts_method == TtsMethod::MagpieMultilingual {
                changed |= render_magpie_settings(ui, config, text);
            } else if config.tts_method == TtsMethod::Kokoro {
                changed |= render_kokoro_settings(ui, config, text);
            } else if config.tts_method == TtsMethod::Supertonic {
                changed |= render_supertonic_settings(ui, config, text);
            } else if config.tts_method == TtsMethod::VieneuTts {
                changed |= render_vieneu_settings(ui, config, text);
            }
        });

    changed
}
