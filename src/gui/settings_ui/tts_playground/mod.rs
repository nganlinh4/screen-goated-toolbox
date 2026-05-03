use super::tts_playground_data::{GEMINI_VOICES, SUPPORTED_GEMINI_INSTRUCTION_LANGUAGES};
use crate::api::tts::TTS_MANAGER;
use crate::api::tts::types::TtsRequestProfile;
use crate::config::{Config, EdgeTtsSettings, EdgeTtsVoiceConfig, TtsLanguageCondition, TtsMethod};
use crate::gui::icons::{Icon, icon_button};
use crate::gui::locale::LocaleText;
use eframe::egui;
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

mod export;
mod library;
mod state;
mod studio;

pub use state::TtsPlaygroundUiState;

static LAST_PLAYGROUND_PREVIEW_IDX: AtomicUsize = AtomicUsize::new(9999);

pub fn render_tts_playground(
    ctx: &egui::Context,
    config: &mut Config,
    text: &LocaleText,
    open: &mut bool,
    state: &mut TtsPlaygroundUiState,
) -> bool {
    if !*open {
        return false;
    }

    let mut changed = false;
    let mut is_open = *open;

    egui::Window::new(format!("{} {}", "🔊", text.tts_playground_title))
        .collapsible(false)
        .resizable(true)
        .default_width(820.0)
        .max_width(820.0)
        .default_height(430.0)
        .min_height(360.0)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .open(&mut is_open)
        .show(ctx, |ui| {
            let available_height = ui.available_height();
            ui.horizontal_top(|ui| {
                let available_width = ui.available_width();
                let right_width = 280.0_f32.min((available_width * 0.42).max(220.0));
                let left_width = (available_width - right_width - 14.0).max(0.0);

                ui.vertical(|ui| {
                    ui.set_width(left_width);
                    egui::ScrollArea::vertical()
                        .id_salt("tts_playground_left_scroll")
                        .max_height(available_height)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            changed |= render_method_picker(ui, config, text);
                            ui.add_space(4.0);

                            match config.tts_playground.method {
                                TtsMethod::GeminiLive => {
                                    changed |= render_gemini_controls(ui, config, text);
                                }
                                TtsMethod::GoogleTranslate => {
                                    changed |= render_google_controls(ui, config, text);
                                }
                                TtsMethod::EdgeTTS => {
                                    changed |= render_edge_controls(ui, config, text);
                                }
                            }
                        });
                });

                ui.separator();

                ui.vertical(|ui| {
                    ui.set_width(right_width);
                    changed |= studio::render_studio_panel(ui, ctx, config, text, state);
                });
            });
        });

    if !is_open {
        studio::stop_player(state);
    }
    *open = is_open;

    changed
}

fn render_method_picker(ui: &mut egui::Ui, config: &mut Config, text: &LocaleText) -> bool {
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new(text.tts_method_label).strong());
        changed |= ui
            .radio_value(
                &mut config.tts_playground.method,
                TtsMethod::GeminiLive,
                text.tts_method_standard,
            )
            .changed();
        changed |= ui
            .radio_value(
                &mut config.tts_playground.method,
                TtsMethod::EdgeTTS,
                text.tts_method_edge,
            )
            .changed();
        changed |= ui
            .radio_value(
                &mut config.tts_playground.method,
                TtsMethod::GoogleTranslate,
                text.tts_method_fast,
            )
            .changed();
    });
    changed
}

fn render_gemini_controls(ui: &mut egui::Ui, config: &mut Config, text: &LocaleText) -> bool {
    let mut changed = false;
    ui.label(egui::RichText::new(text.tts_gemini_model_label).strong());
    ui.horizontal_wrapped(|ui| {
        for (api_model, label) in crate::model_config::tts_gemini_model_options() {
            changed |= ui
                .radio_value(
                    &mut config.tts_playground.gemini_model,
                    (*api_model).to_string(),
                    *label,
                )
                .changed();
        }
    });
    ui.add_space(4.0);

    ui.columns(2, |columns| {
        columns[0].label(egui::RichText::new(text.tts_speed_label).strong());
        changed |= render_speed_radios(
            &mut columns[0],
            &mut config.tts_playground.gemini_speed,
            text,
            true,
        );

        columns[1].label(egui::RichText::new(text.tts_instructions_label).strong());
        changed |= render_gemini_language_conditions(&mut columns[1], config, text);
    });

    ui.add_space(4.0);
    ui.separator();
    ui.add_space(4.0);

    let male_voices: Vec<_> = GEMINI_VOICES.iter().filter(|(_, g)| *g == "Male").collect();
    let female_voices: Vec<_> = GEMINI_VOICES
        .iter()
        .filter(|(_, g)| *g == "Female")
        .collect();

    ui.columns(4, |columns| {
        let male_mid = male_voices.len().div_ceil(2);
        let female_mid = female_voices.len().div_ceil(2);
        let male_col1: Vec<_> = male_voices.iter().take(male_mid).collect();
        let male_col2: Vec<_> = male_voices.iter().skip(male_mid).collect();
        let female_col1: Vec<_> = female_voices.iter().take(female_mid).collect();
        let female_col2: Vec<_> = female_voices.iter().skip(female_mid).collect();

        columns[0].label(egui::RichText::new(text.tts_male).strong().underline());
        columns[0].add_space(2.0);
        for voice in male_col1 {
            changed |= render_gemini_voice(&mut columns[0], voice.0, config, text);
        }

        columns[1].label(egui::RichText::new("").strong());
        columns[1].add_space(2.0);
        for voice in male_col2 {
            changed |= render_gemini_voice(&mut columns[1], voice.0, config, text);
        }

        columns[2].label(egui::RichText::new(text.tts_female).strong().underline());
        columns[2].add_space(2.0);
        for voice in female_col1 {
            changed |= render_gemini_voice(&mut columns[2], voice.0, config, text);
        }

        columns[3].label(egui::RichText::new("").strong());
        columns[3].add_space(2.0);
        for voice in female_col2 {
            changed |= render_gemini_voice(&mut columns[3], voice.0, config, text);
        }
    });
    changed
}

fn render_gemini_voice(
    ui: &mut egui::Ui,
    name: &str,
    config: &mut Config,
    text: &LocaleText,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        let selected = config.tts_playground.gemini_voice == name;
        if ui.radio(selected, "").clicked() {
            config.tts_playground.gemini_voice = name.to_string();
            changed = true;
        }
        if icon_button(ui, Icon::Speaker)
            .on_hover_text(text.tts_preview_label)
            .clicked()
        {
            config.tts_playground.gemini_voice = name.to_string();
            speak_playground_preview(config, text, name);
            changed = true;
        }
        ui.label(egui::RichText::new(name).strong());
    });
    changed
}

fn render_gemini_language_conditions(
    ui: &mut egui::Ui,
    config: &mut Config,
    text: &LocaleText,
) -> bool {
    let mut changed = false;
    let mut to_remove: Option<usize> = None;
    for (idx, condition) in config
        .tts_playground
        .gemini_language_conditions
        .iter_mut()
        .enumerate()
    {
        ui.horizontal(|ui| {
            let display_name = SUPPORTED_GEMINI_INSTRUCTION_LANGUAGES
                .iter()
                .find(|(code, _)| code.eq_ignore_ascii_case(&condition.language_code))
                .map(|(_, name)| *name)
                .unwrap_or(&condition.language_name);

            ui.label(
                egui::RichText::new(display_name)
                    .strong()
                    .color(egui::Color32::from_rgb(100, 180, 100)),
            );
            ui.label("->");
            changed |= ui
                .add(
                    egui::TextEdit::singleline(&mut condition.instruction)
                        .desired_width(180.0)
                        .hint_text(text.tts_instructions_hint),
                )
                .changed();
            if icon_button(ui, Icon::Close)
                .on_hover_text(text.remove_label)
                .clicked()
            {
                to_remove = Some(idx);
            }
        });
    }
    if let Some(idx) = to_remove {
        config.tts_playground.gemini_language_conditions.remove(idx);
        changed = true;
    }

    let used_codes: Vec<_> = config
        .tts_playground
        .gemini_language_conditions
        .iter()
        .map(|condition| condition.language_code.as_str())
        .collect();
    let available: Vec<_> = SUPPORTED_GEMINI_INSTRUCTION_LANGUAGES
        .iter()
        .filter(|(code, _)| !used_codes.contains(code))
        .collect();
    if !available.is_empty() {
        egui::ComboBox::from_id_salt("tts_playground_add_condition")
            .selected_text(text.tts_add_condition)
            .width(150.0)
            .show_ui(ui, |ui| {
                for (code, name) in available {
                    if ui.selectable_label(false, *name).clicked() {
                        config
                            .tts_playground
                            .gemini_language_conditions
                            .push(TtsLanguageCondition::new(code, name, ""));
                        changed = true;
                    }
                }
            });
    }

    changed |= ui
        .add(
            egui::TextEdit::singleline(&mut config.tts_playground.gemini_instruction)
                .desired_width(f32::INFINITY)
                .hint_text(text.tts_playground_instruction_hint),
        )
        .changed();
    changed
}

fn render_google_controls(ui: &mut egui::Ui, config: &mut Config, text: &LocaleText) -> bool {
    render_speed_radios(ui, &mut config.tts_playground.google_speed, text, false)
}

fn render_edge_controls(ui: &mut egui::Ui, config: &mut Config, text: &LocaleText) -> bool {
    crate::api::tts::edge_voices::load_edge_voices_async();
    let mut changed = false;

    ui.vertical_centered(|ui| {
        ui.add_space(4.0);
        ui.label(egui::RichText::new(text.tts_edge_title).size(18.0).strong());
        ui.add_space(2.0);
        ui.label(text.tts_edge_desc);
        ui.add_space(6.0);
    });

    changed |= ui
        .add(
            egui::Slider::new(&mut config.tts_playground.edge_settings.pitch, -50..=50)
                .suffix(" Hz")
                .text(text.tts_pitch_label),
        )
        .changed();
    changed |= ui
        .add(
            egui::Slider::new(&mut config.tts_playground.edge_settings.rate, -50..=100)
                .suffix("%")
                .text(text.tts_rate_label),
        )
        .changed();

    ui.add_space(6.0);
    ui.separator();
    ui.add_space(4.0);
    ui.label(egui::RichText::new(text.tts_voice_per_language_label).strong());
    ui.add_space(2.0);

    let cache_status = {
        let cache = crate::api::tts::edge_voices::EDGE_VOICE_CACHE
            .lock()
            .unwrap();
        (cache.loaded, cache.loading, cache.error.clone())
    };

    if cache_status.1 {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(text.tts_loading_voices);
        });
    } else if let Some(error) = &cache_status.2 {
        ui.colored_label(
            egui::Color32::RED,
            format!("{} {}", text.tts_failed_load_voices, error).replace("{}", ""),
        );
        if ui.button(text.tts_retry_label).clicked() {
            let mut cache = crate::api::tts::edge_voices::EDGE_VOICE_CACHE
                .lock()
                .unwrap();
            cache.loaded = false;
            cache.loading = false;
            cache.error = None;
        }
    } else if cache_status.0 {
        let mut preview_voice: Option<String> = None;
        egui::ScrollArea::vertical()
            .max_height(180.0)
            .show(ui, |ui| {
                let mut to_remove: Option<usize> = None;
                for (idx, voice_config) in config
                    .tts_playground
                    .edge_settings
                    .voice_configs
                    .iter_mut()
                    .enumerate()
                {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(&voice_config.language_name)
                                .strong()
                                .color(egui::Color32::from_rgb(100, 180, 100)),
                        );
                        ui.label("->");
                        let voices = crate::api::tts::edge_voices::get_voices_for_language(
                            &voice_config.language_code,
                        );

                        egui::ComboBox::from_id_salt(format!("tts_playground_edge_voice_{idx}"))
                            .selected_text(&voice_config.voice_name)
                            .width(240.0)
                            .show_ui(ui, |ui| {
                                for voice in &voices {
                                    let display =
                                        format!("{} ({})", voice.short_name, voice.gender);
                                    if ui
                                        .selectable_label(
                                            voice_config.voice_name == voice.short_name,
                                            &display,
                                        )
                                        .clicked()
                                    {
                                        voice_config.voice_name = voice.short_name.clone();
                                        changed = true;
                                    }
                                }
                            });

                        if icon_button(ui, Icon::Speaker)
                            .on_hover_text(text.tts_preview_label)
                            .clicked()
                        {
                            preview_voice = Some(voice_config.voice_name.clone());
                        }
                        if icon_button(ui, Icon::Close)
                            .on_hover_text(text.remove_label)
                            .clicked()
                        {
                            to_remove = Some(idx);
                        }
                    });
                }

                if let Some(idx) = to_remove {
                    config
                        .tts_playground
                        .edge_settings
                        .voice_configs
                        .remove(idx);
                    changed = true;
                }
            });
        if let Some(voice) = preview_voice {
            speak_playground_preview(config, text, &voice);
        }

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let used_codes: Vec<_> = config
                .tts_playground
                .edge_settings
                .voice_configs
                .iter()
                .map(|voice_config| voice_config.language_code.as_str())
                .collect();
            let available_langs = crate::api::tts::edge_voices::get_available_languages();
            let available: Vec<_> = available_langs
                .iter()
                .filter(|(code, _)| !used_codes.contains(&code.as_str()))
                .collect();

            if !available.is_empty() {
                egui::ComboBox::from_id_salt("tts_playground_edge_add_language")
                    .selected_text(text.tts_add_language_label)
                    .width(160.0)
                    .show_ui(ui, |ui| {
                        for (code, name) in &available {
                            if ui.selectable_label(false, name).clicked() {
                                let voices =
                                    crate::api::tts::edge_voices::get_voices_for_language(code);
                                let default_voice = voices
                                    .first()
                                    .map(|voice| voice.short_name.clone())
                                    .unwrap_or_else(|| format!("{code}-??-??Neural"));
                                config.tts_playground.edge_settings.voice_configs.push(
                                    EdgeTtsVoiceConfig {
                                        language_code: code.clone(),
                                        language_name: name.clone(),
                                        voice_name: default_voice,
                                    },
                                );
                                changed = true;
                            }
                        }
                    });
            }

            if ui.button(text.tts_reset_to_defaults_label).clicked() {
                config.tts_playground.edge_settings = EdgeTtsSettings::default();
                changed = true;
            }
        });
    } else {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(text.tts_initializing_voices);
        });
    }

    if let Some(first_voice) = config.tts_playground.edge_settings.voice_configs.first() {
        config.tts_playground.edge_voice = first_voice.voice_name.clone();
        config.tts_playground.edge_pitch = config.tts_playground.edge_settings.pitch;
        config.tts_playground.edge_rate = config.tts_playground.edge_settings.rate;
    }

    changed
}

fn speak_playground_preview(config: &Config, text: &LocaleText, speaker_name: &str) {
    let preview_text = random_preview_text(text, speaker_name);
    let profile = TtsRequestProfile::from(&config.tts_playground);
    TTS_MANAGER.speak_interrupt_with_profile(&preview_text, 0, profile);
}

fn random_preview_text(text: &LocaleText, speaker_name: &str) -> String {
    if text.tts_preview_texts.is_empty() {
        return format!("Hello, I am {speaker_name}. This is a voice preview.");
    }

    let s = RandomState::new();
    let mut hasher = s.build_hasher();
    hasher.write_usize(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos() as usize,
    );
    let len = text.tts_preview_texts.len();
    let mut idx = (hasher.finish() as usize) % len;
    let last = LAST_PLAYGROUND_PREVIEW_IDX.load(Ordering::Relaxed);
    if idx == last {
        idx = (idx + 1) % len;
    }
    LAST_PLAYGROUND_PREVIEW_IDX.store(idx, Ordering::Relaxed);
    text.tts_preview_texts[idx].replace("{}", speaker_name)
}

fn render_speed_radios(
    ui: &mut egui::Ui,
    speed: &mut String,
    text: &LocaleText,
    allow_fast: bool,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        changed |= ui
            .radio_value(speed, "Slow".to_string(), text.tts_speed_slow)
            .changed();
        changed |= ui
            .radio_value(speed, "Normal".to_string(), text.tts_speed_normal)
            .changed();
        if allow_fast {
            changed |= ui
                .radio_value(speed, "Fast".to_string(), text.tts_speed_fast)
                .changed();
        }
    });
    changed
}
