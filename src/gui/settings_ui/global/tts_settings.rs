use crate::config::tts_catalog::{
    KOKORO_VOICE_LANGUAGES, KOKORO_VOICES, MAGPIE_VOICE_LANGUAGES, MAGPIE_VOICES,
    SUPERTONIC_LANGUAGES, SUPERTONIC_VOICES, default_kokoro_voice_for_lang,
    default_magpie_voice_for_lang, default_supertonic_voice_for_lang,
    kokoro_voice_language_for_condition, normalize_magpie_voice, normalize_supertonic_lang,
    normalize_supertonic_voice,
};
use crate::config::{
    Config, KokoroVoiceConfig, MagpieVoiceConfig, SupertonicVoiceConfig, TtsMethod,
};
use crate::gui::icons::{Icon, icon_button};
use crate::gui::locale::LocaleText;
use eframe::egui;
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static LAST_TTS_PREVIEW_IDX: AtomicUsize = AtomicUsize::new(9999);

const SUPERTONIC_LANGUAGE_SUMMARY: &str = "Supports English, Korean, Japanese, Arabic, Bulgarian, Czech, Danish, German, Greek, Spanish, Estonian, Finnish, French, Hindi, Croatian, Hungarian, Indonesian, Italian, Lithuanian, Latvian, Dutch, Polish, Portuguese, Romanian, Russian, Slovak, Slovenian, Swedish, Turkish, Ukrainian, and Vietnamese.";

fn speak_settings_preview(text: &LocaleText, speaker_name: &str) {
    if !text.tts_preview_texts.is_empty() {
        let s = RandomState::new();
        let mut hasher = s.build_hasher();
        hasher.write_usize(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .subsec_nanos() as usize,
        );
        let rand_val = hasher.finish();
        let len = text.tts_preview_texts.len();
        let mut idx = (rand_val as usize) % len;

        let last = LAST_TTS_PREVIEW_IDX.load(Ordering::Relaxed);
        if idx == last {
            idx = (idx + 1) % len;
        }
        LAST_TTS_PREVIEW_IDX.store(idx, Ordering::Relaxed);

        let preview_text = text.tts_preview_texts[idx].replace("{}", speaker_name);
        crate::api::tts::TTS_MANAGER.speak_interrupt(&preview_text, 0);
    } else {
        let preview_text = format!("Hello, I am {}. This is a voice preview.", speaker_name);
        crate::api::tts::TTS_MANAGER.speak_interrupt(&preview_text, 0);
    }
}

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

    // List of voices (Name, Gender)
    const VOICES: &[(&str, &str)] = &[
        ("Achernar", "Female"),
        ("Achird", "Male"),
        ("Algenib", "Male"),
        ("Algieba", "Male"),
        ("Alnilam", "Male"),
        ("Aoede", "Female"),
        ("Autonoe", "Female"),
        ("Callirrhoe", "Female"),
        ("Charon", "Male"),
        ("Despina", "Female"),
        ("Enceladus", "Male"),
        ("Erinome", "Female"),
        ("Fenrir", "Male"),
        ("Gacrux", "Female"),
        ("Iapetus", "Male"),
        ("Kore", "Female"),
        ("Laomedeia", "Female"),
        ("Leda", "Female"),
        ("Orus", "Male"),
        ("Pulcherrima", "Female"),
        ("Puck", "Male"),
        ("Rasalgethi", "Male"),
        ("Sadachbia", "Male"),
        ("Sadaltager", "Male"),
        ("Schedar", "Male"),
        ("Sulafat", "Female"),
        ("Umbriel", "Male"),
        ("Vindemiatrix", "Female"),
        ("Zephyr", "Female"),
        ("Zubenelgenubi", "Male"),
    ];

    let male_voices: Vec<_> = VOICES.iter().filter(|(_, g)| *g == "Male").collect();
    let female_voices: Vec<_> = VOICES.iter().filter(|(_, g)| *g == "Female").collect();

    egui::Window::new(format!("🔊 {}", text.tts_settings_title))
        .collapsible(false)
        .resizable(false)
        .title_bar(false)
        .default_width(650.0)
        .default_height(600.0)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ui.ctx(), |ui| {
            ui.set_min_height(500.0); // Force minimum height for the content area

            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(format!("🔊 {}", text.tts_settings_title)).strong().size(14.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if icon_button(ui, Icon::Close).clicked() {
                        *show_modal = false;
                    }
                });
            });
            ui.separator();
            ui.add_space(8.0);

            if config.tts_method == TtsMethod::VoxtralTts {
                config.tts_method = TtsMethod::VieneuTts;
                changed = true;
            }

            // === TTS METHOD SELECTION ===
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new(text.tts_method_label).strong());

                // Gemini Live (Premium)
                if ui.radio_value(&mut config.tts_method, TtsMethod::GeminiLive, text.tts_method_standard).clicked() {
                    changed = true;
                }

                // Edge TTS (Good)
                if ui.radio_value(&mut config.tts_method, TtsMethod::EdgeTTS, text.tts_method_edge).clicked() {
                    changed = true;
                }

                // Google Translate (Fast)
                if ui.radio_value(&mut config.tts_method, TtsMethod::GoogleTranslate, text.tts_method_fast).clicked() {
                    if config.tts_speed == "Fast" {
                        config.tts_speed = "Normal".to_string();
                    }
                    changed = true;
                }

                // Open-weights leaderboard providers
                if ui
                    .radio_value(&mut config.tts_method, TtsMethod::StepAudioEditX, "Step Audio EditX")
                    .on_hover_text("Supports Mandarin, English, Sichuanese, Cantonese, Japanese, and Korean.")
                    .clicked()
                {
                    changed = true;
                }
                if ui
                    .radio_value(
                        &mut config.tts_method,
                        TtsMethod::MagpieMultilingual,
                        "NVIDIA Magpie-Multilingual 357M",
                    )
                    .on_hover_text("Supports English, Spanish, German, French, Vietnamese, Italian, Mandarin Chinese, Hindi, and Japanese.")
                    .clicked()
                {
                    changed = true;
                }
                if ui
                    .radio_value(&mut config.tts_method, TtsMethod::Kokoro, "Kokoro 82M v1.0")
                    .on_hover_text("Supports English, Mandarin Chinese, Japanese, Spanish, French, Hindi, Italian, and Portuguese.")
                    .clicked()
                {
                    changed = true;
                }
                if ui
                    .radio_value(&mut config.tts_method, TtsMethod::Supertonic, "Supertonic 3")
                    .on_hover_text(SUPERTONIC_LANGUAGE_SUMMARY)
                    .clicked()
                {
                    changed = true;
                }
                if ui
                    .radio_value(&mut config.tts_method, TtsMethod::VieneuTts, "VieNeu-TTS v2")
                    .on_hover_text("Vietnamese-first local TTS with English/Vietnamese code-switching and zero-shot voice cloning.")
                    .clicked()
                {
                    changed = true;
                }
            });
            ui.add_space(10.0);
            ui.separator();
            ui.add_space(10.0);

            // Speed and Tone & Style side by side
            if config.tts_method == TtsMethod::GeminiLive {
                ui.label(egui::RichText::new(text.tts_gemini_model_label).strong());
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
                    columns[0].label(egui::RichText::new(text.tts_speed_label).strong());
                    columns[0].horizontal(|ui| {
                        if ui.radio_value(&mut config.tts_speed, "Slow".to_string(), text.tts_speed_slow).clicked() { changed = true; }
                        if ui.radio_value(&mut config.tts_speed, "Normal".to_string(), text.tts_speed_normal).clicked() { changed = true; }
                        if ui.radio_value(&mut config.tts_speed, "Fast".to_string(), text.tts_speed_fast).clicked() { changed = true; }
                    });

                    // Right column: Language-Specific Instructions
                    columns[1].label(egui::RichText::new(text.tts_instructions_label).strong());

                    // Supported languages are mapped from ISO 639-3 codes
                    let supported_languages = [
                        ("afr", "Afrikaans"), ("ara", "Arabic"), ("aze", "Azerbaijani"),
                        ("bel", "Belarusian"), ("ben", "Bengali"), ("bul", "Bulgarian"),
                        ("cat", "Catalan"), ("ces", "Czech"), ("cmn", "Mandarin Chinese"),
                        ("dan", "Danish"), ("deu", "German"), ("ell", "Greek"),
                        ("eng", "English"), ("epo", "Esperanto"), ("est", "Estonian"),
                        ("eus", "Basque"), ("fin", "Finnish"), ("fra", "French"),
                        ("guj", "Gujarati"), ("heb", "Hebrew"), ("hin", "Hindi"),
                        ("hrv", "Croatian"), ("hun", "Hungarian"), ("ind", "Indonesian"),
                        ("ita", "Italian"), ("jpn", "Japanese"), ("kan", "Kannada"),
                        ("kat", "Georgian"), ("kor", "Korean"), ("lat", "Latin"),
                        ("lav", "Latvian"), ("lit", "Lithuanian"), ("mal", "Malayalam"),
                        ("mar", "Marathi"), ("mkd", "Macedonian"), ("mya", "Burmese"),
                        ("nep", "Nepali"), ("nld", "Dutch"), ("nno", "Norwegian Nynorsk"),
                        ("nob", "Norwegian Bokmål"), ("ori", "Oriya"), ("pan", "Punjabi"),
                        ("pes", "Persian"), ("pol", "Polish"), ("por", "Portuguese"),
                        ("ron", "Romanian"), ("rus", "Russian"), ("sin", "Sinhala"),
                        ("slk", "Slovak"), ("slv", "Slovenian"), ("som", "Somali"),
                        ("spa", "Spanish"), ("sqi", "Albanian"), ("srp", "Serbian"),
                        ("swe", "Swedish"), ("tam", "Tamil"), ("tel", "Telugu"),
                        ("tgl", "Tagalog"), ("tha", "Thai"), ("tur", "Turkish"),
                        ("ukr", "Ukrainian"), ("urd", "Urdu"), ("uzb", "Uzbek"),
                        ("vie", "Vietnamese"), ("yid", "Yiddish"), ("zho", "Chinese"),
                    ];

                    // Show existing conditions
                    let mut to_remove: Option<usize> = None;
                    for (idx, condition) in config.tts_language_conditions.iter_mut().enumerate() {
                        columns[1].horizontal(|ui| {
                            // Language dropdown (read-only display for now)
                            let display_name = supported_languages.iter()
                                .find(|(code, _)| code.eq_ignore_ascii_case(&condition.language_code))
                                .map(|(_, name)| *name)
                                .unwrap_or(&condition.language_name);

                            ui.label(egui::RichText::new(display_name).strong().color(egui::Color32::from_rgb(100, 180, 100)));
                            ui.label("→");

                            // Instruction input
                            if ui.add(
                                egui::TextEdit::singleline(&mut condition.instruction)
                                    .desired_width(180.0)
                                    .hint_text(text.tts_instructions_hint)
                            ).changed() {
                                changed = true;
                            }

                            // Remove button - use Icon::Close for proper rendering
                            if icon_button(ui, Icon::Close).on_hover_text(text.remove_label).clicked() {
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
                            egui::ComboBox::from_id_salt("tts_add_condition")
                                .selected_text(text.tts_add_condition)
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
                                .on_hover_text(text.tts_preview_label)
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
                        ui.label(egui::RichText::new(text.tts_male).strong().underline());
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
                        ui.label(egui::RichText::new(text.tts_female).strong().underline());
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
                    ui.label(egui::RichText::new(text.tts_google_translate_title).size(18.0).strong());
                    ui.add_space(10.0);
                    ui.label(text.tts_google_translate_desc);
                    ui.add_space(20.0);

                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(text.tts_speed_label).strong());
                        if ui.radio_value(&mut config.tts_speed, "Slow".to_string(), text.tts_speed_slow).clicked() { changed = true; }
                        if ui.radio_value(&mut config.tts_speed, "Normal".to_string(), text.tts_speed_normal).clicked() { changed = true; }
                    });

                    ui.add_space(12.0);
                    if icon_button(ui, Icon::Speaker)
                        .on_hover_text(text.tts_preview_label)
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
                    ui.label(egui::RichText::new(text.tts_edge_title).size(18.0).strong());
                    ui.add_space(5.0);
                    ui.label(text.tts_edge_desc);
                    ui.add_space(15.0);
                });

                // Pitch and Rate sliders
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(text.tts_pitch_label).strong());
                    if ui.add(egui::Slider::new(&mut config.edge_tts_settings.pitch, -50..=50).suffix(" Hz")).changed() {
                        changed = true;
                    }
                });

                ui.add_space(5.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(text.tts_rate_label).strong());
                    if ui.add(egui::Slider::new(&mut config.edge_tts_settings.rate, -50..=100).suffix("%")).changed() {
                        changed = true;
                    }
                });

                ui.add_space(15.0);
                ui.separator();
                ui.add_space(10.0);

                // Per-language voice configuration
                ui.label(egui::RichText::new(text.tts_voice_per_language_label).strong());
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
                        ui.label(text.tts_loading_voices);
                    });
                } else if let Some(ref error) = cache_status.2 {
                    // Error
                    ui.colored_label(egui::Color32::RED, format!("{} {}", text.tts_failed_load_voices, error).replace("{}", ""));
                    if ui.button(text.tts_retry_label).clicked() {
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
                                ui.label(egui::RichText::new(&voice_config.language_name).strong().color(egui::Color32::from_rgb(100, 180, 100)));
                                ui.label("→");

                                // Voice dropdown for this language
                                let voices = crate::api::tts::edge_voices::get_voices_for_language(&voice_config.language_code);

                                egui::ComboBox::from_id_salt(format!("edge_voice_{}", idx))
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
                                    .on_hover_text(text.tts_preview_label)
                                    .clicked()
                                {
                                    speak_settings_preview(text, &voice_config.voice_name);
                                }

                                // Remove button
                                if icon_button(ui, Icon::Close).on_hover_text(text.remove_label).clicked() {
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
                            egui::ComboBox::from_id_salt("edge_add_language")
                                .selected_text(text.tts_add_language_label)
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

                        if ui.button(text.tts_reset_to_defaults_label).clicked() {
                            config.edge_tts_settings = crate::config::EdgeTtsSettings::default();
                            config.tts_playground = crate::config::TtsPlaygroundSettings::default();
                            changed = true;
                        }
                    });
                } else {
                    // Not loaded yet, show loading message
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(text.tts_initializing_voices);
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

// ============================================================================
// OPEN-WEIGHTS PROVIDER SETTINGS PANELS
// ============================================================================
//
// Each provider exposes the same shape: base URL, optional API key, voice/id,
// speed slider, plus any provider-specific knobs. We render them inline rather
// than in sub-modals to keep navigation flat — the user picks the method radio
// at the top, the matching block appears below.

fn render_open_weights_header(ui: &mut egui::Ui, title: &str, description: &str) {
    ui.vertical_centered(|ui| {
        ui.add_space(10.0);
        ui.label(egui::RichText::new(title).size(18.0).strong());
        ui.add_space(5.0);
        ui.label(description);
        ui.add_space(15.0);
    });
}

fn render_speed_row(ui: &mut egui::Ui, label: &str, value: &mut f32, min: f32, max: f32) -> bool {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).strong());
        ui.add(egui::Slider::new(value, min..=max).step_by(0.05))
    })
    .inner
    .changed()
}

fn render_step_audio_settings(ui: &mut egui::Ui, config: &mut Config, text: &LocaleText) -> bool {
    let mut changed = false;
    render_open_weights_header(ui, "Step Audio EditX", text.tts_step_audio_desc);
    changed |= render_step_audio_reference_controls(ui, config, text);
    changed
}

fn render_step_audio_reference_controls(
    ui: &mut egui::Ui,
    config: &mut Config,
    text: &LocaleText,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(text.tts_reference_voice_label);
        let selected = config
            .step_audio_reference_voices
            .iter()
            .find(|item| item.id == config.step_audio_settings.reference_voice_id)
            .map(|item| reference_label_or_default(item, text))
            .unwrap_or_else(|| text.tts_reference_default.to_string());
        egui::ComboBox::from_id_salt("step_audio_global_reference_voice")
            .selected_text(selected)
            .width(240.0)
            .show_ui(ui, |ui| {
                changed |= ui
                    .selectable_value(
                        &mut config.step_audio_settings.reference_voice_id,
                        String::new(),
                        text.tts_reference_default,
                    )
                    .changed();
                for reference in &config.step_audio_reference_voices {
                    changed |= ui
                        .selectable_value(
                            &mut config.step_audio_settings.reference_voice_id,
                            reference.id.clone(),
                            reference_label_or_default(reference, text),
                        )
                        .changed();
                }
            });
    });
    changed
}

fn render_magpie_settings(ui: &mut egui::Ui, config: &mut Config, text: &LocaleText) -> bool {
    let mut changed = false;
    render_open_weights_header(
        ui,
        "NVIDIA Magpie-Multilingual 357M",
        "Supports English, Spanish, German, French, Vietnamese, Italian, Mandarin Chinese, Hindi, and Japanese.",
    );
    changed |= render_magpie_voice_config_rows(ui, config, text);
    changed
}

fn render_magpie_voice_config_rows(
    ui: &mut egui::Ui,
    config: &mut Config,
    text: &LocaleText,
) -> bool {
    let mut changed = false;
    ui.add_space(15.0);
    ui.separator();
    ui.add_space(10.0);
    ui.label(egui::RichText::new(text.tts_voice_per_language_label).strong());
    ui.add_space(5.0);

    egui::ScrollArea::vertical()
        .max_height(180.0)
        .show(ui, |ui| {
            let mut to_remove: Option<usize> = None;
            for (idx, voice_config) in config.magpie_settings.voice_configs.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(&voice_config.language_name)
                            .strong()
                            .color(egui::Color32::from_rgb(100, 180, 100)),
                    );
                    ui.label("→");
                    egui::ComboBox::from_id_salt(format!("magpie_voice_{}", idx))
                        .selected_text(normalize_magpie_voice(&voice_config.voice_id))
                        .width(220.0)
                        .show_ui(ui, |ui| {
                            for voice in MAGPIE_VOICES {
                                changed |= ui
                                    .selectable_value(
                                        &mut voice_config.voice_id,
                                        voice.id.to_string(),
                                        voice.label,
                                    )
                                    .changed();
                            }
                        });
                    if icon_button(ui, Icon::Close)
                        .on_hover_text(text.remove_label)
                        .clicked()
                    {
                        to_remove = Some(idx);
                    }
                });
            }
            if let Some(idx) = to_remove {
                config.magpie_settings.voice_configs.remove(idx);
                changed = true;
            }
        });

    ui.add_space(10.0);
    ui.horizontal(|ui| {
        let used_codes: Vec<_> = config
            .magpie_settings
            .voice_configs
            .iter()
            .map(|voice_config| voice_config.language_code.as_str())
            .collect();
        let available: Vec<_> = MAGPIE_VOICE_LANGUAGES
            .iter()
            .filter(|(code, _)| !used_codes.contains(code))
            .collect();

        if !available.is_empty() {
            egui::ComboBox::from_id_salt("magpie_add_language")
                .selected_text(text.tts_add_language_label)
                .width(150.0)
                .show_ui(ui, |ui| {
                    for (code, name) in &available {
                        if ui.selectable_label(false, *name).clicked() {
                            config
                                .magpie_settings
                                .voice_configs
                                .push(MagpieVoiceConfig::new(
                                    code,
                                    name,
                                    default_magpie_voice_for_lang(code),
                                ));
                            changed = true;
                        }
                    }
                });
        }

        if ui.button(text.tts_reset_to_defaults_label).clicked() {
            config.magpie_settings.voice_configs =
                crate::config::MagpieSettings::default().voice_configs;
            changed = true;
        }
    });
    changed
}

fn render_kokoro_settings(ui: &mut egui::Ui, config: &mut Config, text: &LocaleText) -> bool {
    let mut changed = false;
    render_open_weights_header(ui, text.tts_kokoro_title, text.tts_kokoro_desc);
    let s = &mut config.kokoro_settings;
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(text.tts_cpu_threads_label).strong());
        changed |= ui
            .add(egui::Slider::new(&mut s.num_threads, 1..=8))
            .changed();
    });
    changed |= render_speed_row(ui, text.tts_speed_label, &mut s.speed, 0.5, 2.0);
    changed |= render_kokoro_voice_config_rows(ui, config, text);
    changed
}

fn render_kokoro_voice_config_rows(
    ui: &mut egui::Ui,
    config: &mut Config,
    text: &LocaleText,
) -> bool {
    let mut changed = false;
    ui.add_space(15.0);
    ui.separator();
    ui.add_space(10.0);
    ui.label(egui::RichText::new(text.tts_voice_per_language_label).strong());
    ui.add_space(5.0);

    egui::ScrollArea::vertical()
        .max_height(180.0)
        .show(ui, |ui| {
            let mut to_remove: Option<usize> = None;
            for (idx, voice_config) in config.kokoro_settings.voice_configs.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(&voice_config.language_name)
                            .strong()
                            .color(egui::Color32::from_rgb(100, 180, 100)),
                    );
                    ui.label("→");

                    let voice_lang =
                        kokoro_voice_language_for_condition(&voice_config.language_code)
                            .unwrap_or("en-us");
                    egui::ComboBox::from_id_salt(format!("kokoro_voice_{idx}"))
                        .selected_text(&voice_config.voice_id)
                        .width(220.0)
                        .show_ui(ui, |ui| {
                            for voice in KOKORO_VOICES
                                .iter()
                                .filter(|voice| voice.language_code == voice_lang)
                            {
                                let display = format!("{} ({})", voice.id, voice.label);
                                if ui
                                    .selectable_label(voice_config.voice_id == voice.id, &display)
                                    .clicked()
                                {
                                    voice_config.voice_id = voice.id.to_string();
                                    changed = true;
                                }
                            }
                        });

                    if icon_button(ui, Icon::Speaker)
                        .on_hover_text(text.tts_preview_label)
                        .clicked()
                    {
                        speak_settings_preview(text, &voice_config.voice_id);
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
                config.kokoro_settings.voice_configs.remove(idx);
                changed = true;
            }
        });

    ui.add_space(10.0);
    ui.horizontal(|ui| {
        let used_codes: Vec<_> = config
            .kokoro_settings
            .voice_configs
            .iter()
            .map(|voice_config| voice_config.language_code.as_str())
            .collect();
        let available: Vec<_> = KOKORO_VOICE_LANGUAGES
            .iter()
            .filter(|(code, _)| !used_codes.contains(code))
            .collect();

        if !available.is_empty() {
            egui::ComboBox::from_id_salt("kokoro_add_language")
                .selected_text(text.tts_add_language_label)
                .width(150.0)
                .show_ui(ui, |ui| {
                    for (code, name) in &available {
                        if ui.selectable_label(false, *name).clicked() {
                            let voice_lang =
                                kokoro_voice_language_for_condition(code).unwrap_or("en-us");
                            config
                                .kokoro_settings
                                .voice_configs
                                .push(KokoroVoiceConfig {
                                    language_code: (*code).to_string(),
                                    language_name: (*name).to_string(),
                                    voice_id: default_kokoro_voice_for_lang(voice_lang).to_string(),
                                });
                            changed = true;
                        }
                    }
                });
        }

        if ui.button(text.tts_reset_to_defaults_label).clicked() {
            config.kokoro_settings.voice_configs =
                crate::config::KokoroSettings::default().voice_configs;
            changed = true;
        }
    });

    changed
}

fn render_supertonic_settings(ui: &mut egui::Ui, config: &mut Config, text: &LocaleText) -> bool {
    let mut changed = false;
    render_open_weights_header(ui, "Supertonic 3", SUPERTONIC_LANGUAGE_SUMMARY);
    let s = &mut config.supertonic_settings;
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(text.tts_cpu_threads_label).strong());
        changed |= ui
            .add(egui::Slider::new(&mut s.num_threads, 1..=8))
            .changed();
    });
    changed |= render_speed_row(ui, text.tts_speed_label, &mut s.speed, 0.5, 2.0);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(text.tts_quality_steps_label).strong());
        changed |= ui
            .add(egui::Slider::new(&mut s.num_steps, 1..=20))
            .changed();
    });
    changed |= render_supertonic_voice_config_rows(ui, config, text);
    changed
}

fn render_supertonic_voice_config_rows(
    ui: &mut egui::Ui,
    config: &mut Config,
    text: &LocaleText,
) -> bool {
    let mut changed = false;
    ui.add_space(15.0);
    ui.separator();
    ui.add_space(10.0);
    ui.label(egui::RichText::new(text.tts_voice_per_language_label).strong());

    egui::ScrollArea::vertical()
        .max_height(180.0)
        .show(ui, |ui| {
            let mut to_remove: Option<usize> = None;
            for (idx, voice_config) in config
                .supertonic_settings
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
                    ui.label("→");
                    egui::ComboBox::from_id_salt(format!("supertonic_voice_{idx}"))
                        .selected_text(normalize_supertonic_voice(&voice_config.voice_id))
                        .width(160.0)
                        .show_ui(ui, |ui| {
                            for voice in SUPERTONIC_VOICES {
                                changed |= ui
                                    .selectable_value(
                                        &mut voice_config.voice_id,
                                        voice.id.to_string(),
                                        voice.label,
                                    )
                                    .changed();
                            }
                        });
                    if icon_button(ui, Icon::Close)
                        .on_hover_text(text.remove_label)
                        .clicked()
                    {
                        to_remove = Some(idx);
                    }
                });
            }
            if let Some(idx) = to_remove {
                config.supertonic_settings.voice_configs.remove(idx);
                changed = true;
            }
        });

    ui.add_space(10.0);
    ui.horizontal(|ui| {
        let used_codes: Vec<_> = config
            .supertonic_settings
            .voice_configs
            .iter()
            .filter_map(|voice_config| normalize_supertonic_lang(&voice_config.language_code))
            .collect();
        let available: Vec<_> = SUPERTONIC_LANGUAGES
            .iter()
            .filter(|lang| !used_codes.iter().any(|code| code == lang.code))
            .collect();

        if !available.is_empty() {
            egui::ComboBox::from_id_salt("supertonic_add_language")
                .selected_text(text.tts_add_language_label)
                .width(150.0)
                .show_ui(ui, |ui| {
                    for lang in &available {
                        if ui.selectable_label(false, lang.label).clicked() {
                            config.supertonic_settings.voice_configs.push(
                                SupertonicVoiceConfig::new(
                                    lang.code,
                                    lang.label,
                                    default_supertonic_voice_for_lang(lang.code),
                                ),
                            );
                            changed = true;
                        }
                    }
                });
        }

        if ui.button(text.tts_reset_to_defaults_label).clicked() {
            config.supertonic_settings.voice_configs =
                crate::config::SupertonicSettings::default().voice_configs;
            changed = true;
        }
    });

    changed
}

fn render_vieneu_settings(ui: &mut egui::Ui, config: &mut Config, text: &LocaleText) -> bool {
    let mut changed = false;
    render_open_weights_header(ui, "VieNeu-TTS v2", text.tts_vieneu_desc);
    ui.label(
        egui::RichText::new(text.tts_vieneu_control_desc)
            .small()
            .color(egui::Color32::from_rgb(96, 125, 139)),
    );
    changed |= render_vieneu_reference_controls(ui, config, text);
    changed
}

fn render_vieneu_reference_controls(
    ui: &mut egui::Ui,
    config: &mut Config,
    text: &LocaleText,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(text.tts_reference_voice_label);
        let selected = config
            .step_audio_reference_voices
            .iter()
            .find(|item| item.id == config.vieneu_settings.reference_voice_id)
            .map(|item| reference_label_or_default(item, text))
            .unwrap_or_else(|| text.tts_reference_default.to_string());
        egui::ComboBox::from_id_salt("vieneu_global_reference_voice")
            .selected_text(selected)
            .width(240.0)
            .show_ui(ui, |ui| {
                changed |= ui
                    .selectable_value(
                        &mut config.vieneu_settings.reference_voice_id,
                        String::new(),
                        text.tts_reference_default,
                    )
                    .changed();
                for reference in &config.step_audio_reference_voices {
                    changed |= ui
                        .selectable_value(
                            &mut config.vieneu_settings.reference_voice_id,
                            reference.id.clone(),
                            reference_label_or_default(reference, text),
                        )
                        .changed();
                }
            });
    });
    changed
}

fn reference_label_or_default(
    reference: &crate::config::StepAudioReferenceVoice,
    text: &LocaleText,
) -> String {
    if reference.label.trim().is_empty() {
        text.tts_reference_untitled.to_string()
    } else {
        reference.label.clone()
    }
}
