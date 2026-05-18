use super::tts_playground_data::{GEMINI_VOICES, SUPPORTED_GEMINI_INSTRUCTION_LANGUAGES};
use crate::api::tts::TTS_MANAGER;
use crate::api::tts::types::TtsRequestProfile;
use crate::config::tts_catalog::{
    KOKORO_VOICE_LANGUAGES, KOKORO_VOICES, MAGPIE_VOICE_LANGUAGES, MAGPIE_VOICES,
    SUPERTONIC_LANGUAGES, SUPERTONIC_VOICES, default_kokoro_voice_for_lang,
    default_magpie_voice_for_lang, default_supertonic_voice_for_lang,
    kokoro_voice_language_for_condition, normalize_magpie_voice, normalize_supertonic_lang,
    normalize_supertonic_voice,
};
use crate::config::{
    Config, EdgeTtsSettings, EdgeTtsVoiceConfig, KokoroVoiceConfig, MagpieVoiceConfig,
    SupertonicVoiceConfig, TtsLanguageCondition, TtsMethod, TtsPlaygroundMode,
    TtsPlaygroundSettings,
};
use crate::gui::icons::{Icon, icon_button};
use crate::gui::locale::LocaleText;
use eframe::egui;
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

mod export;
mod library;
mod reference_library;
mod state;
mod studio;

use state::MicRecordingState;
pub use state::TtsPlaygroundUiState;

static LAST_PLAYGROUND_PREVIEW_IDX: AtomicUsize = AtomicUsize::new(9999);
const SUPERTONIC_LANGUAGE_SUMMARY: &str = "Supports English, Korean, Japanese, Arabic, Bulgarian, Czech, Danish, German, Greek, Spanish, Estonian, Finnish, French, Hindi, Croatian, Hungarian, Indonesian, Italian, Lithuanian, Latvian, Dutch, Polish, Portuguese, Romanian, Russian, Slovak, Slovenian, Swedish, Turkish, Ukrainian, and Vietnamese.";

pub fn pick_step_audio_reference_audio() -> Result<Option<std::path::PathBuf>, String> {
    export::pick_audio_file_dialog()
}

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

    let window_response = egui::Window::new(format!("{} {}", "🔊", text.tts_playground_title))
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
                            changed |= render_mode_tabs(ui, config);
                            ui.add_space(4.0);
                            match config.tts_playground.mode {
                                TtsPlaygroundMode::TtsClone => {
                                    changed |= render_method_picker(ui, config, text);
                                    ui.add_space(4.0);
                                    match config.tts_playground.method {
                                        TtsMethod::GeminiLive => {
                                            changed |= render_gemini_controls(ui, config, text);
                                        }
                                        TtsMethod::GoogleTranslate => {
                                            changed |= render_google_controls(ui, config, text);
                                        }
                                        TtsMethod::FishAudioS2Pro => {
                                            config.tts_playground.method = TtsMethod::GeminiLive;
                                            changed = true;
                                        }
                                        TtsMethod::EdgeTTS => {
                                            changed |= render_edge_controls(ui, config, text);
                                        }
                                        TtsMethod::StepAudioEditX => {
                                            changed |= render_step_audio_controls(ui, config);
                                        }
                                        TtsMethod::MagpieMultilingual => {
                                            changed |= render_magpie_controls(ui, config);
                                        }
                                        TtsMethod::Kokoro => {
                                            changed |= render_kokoro_controls(ui, config, text);
                                        }
                                        TtsMethod::Supertonic => {
                                            changed |= render_supertonic_controls(ui, config, text);
                                        }
                                        TtsMethod::VieneuTts => {
                                            changed |= render_vieneu_controls(ui, config);
                                        }
                                        TtsMethod::VoxtralTts => {
                                            changed |= render_voxtral_controls(ui, config);
                                        }
                                    }
                                }
                                TtsPlaygroundMode::AudioEdit => {
                                    changed |= render_step_audio_edit_controls(ui, config, state);
                                }
                                TtsPlaygroundMode::ReferenceLibrary => {
                                    changed |= reference_library::render_reference_library(
                                        ui, config, state,
                                    );
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
    if window_response
        .as_ref()
        .is_some_and(|inner| inner.response.hovered())
    {
        ctx.data_mut(|data| {
            data.insert_temp(egui::Id::new("tts_playground_hovered"), true);
        });
    }

    if !is_open {
        studio::stop_player(state);
    }
    *open = is_open;

    changed
}

fn render_mode_tabs(ui: &mut egui::Ui, config: &mut Config) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        if ui
            .selectable_value(
                &mut config.tts_playground.mode,
                TtsPlaygroundMode::TtsClone,
                "TTS / Clone",
            )
            .changed()
        {
            changed = true;
        }
        if ui
            .selectable_value(
                &mut config.tts_playground.mode,
                TtsPlaygroundMode::AudioEdit,
                "Audio Edit",
            )
            .changed()
        {
            changed = true;
        }
        if ui
            .selectable_value(
                &mut config.tts_playground.mode,
                TtsPlaygroundMode::ReferenceLibrary,
                "Reference voice library",
            )
            .changed()
        {
            if config.tts_playground.draft_text.trim().is_empty()
                || config.tts_playground.draft_text == TtsPlaygroundSettings::default().draft_text
            {
                config.tts_playground.draft_text = reference_library::TAG_EXAMPLE_TEXT.to_string();
            }
            changed = true;
        }
    });
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
        changed |= ui
            .radio_value(
                &mut config.tts_playground.method,
                TtsMethod::StepAudioEditX,
                "Step Audio EditX",
            )
            .changed();
        changed |= ui
            .radio_value(
                &mut config.tts_playground.method,
                TtsMethod::MagpieMultilingual,
                "NVIDIA Magpie-Multilingual 357M",
            )
            .changed();
        changed |= ui
            .radio_value(
                &mut config.tts_playground.method,
                TtsMethod::Kokoro,
                "Kokoro 82M v1.0",
            )
            .changed();
        changed |= ui
            .radio_value(
                &mut config.tts_playground.method,
                TtsMethod::Supertonic,
                "Supertonic 3",
            )
            .changed();
        changed |= ui
            .radio_value(
                &mut config.tts_playground.method,
                TtsMethod::VieneuTts,
                "VieNeu-TTS v2",
            )
            .changed();
        changed |= ui
            .radio_value(
                &mut config.tts_playground.method,
                TtsMethod::VoxtralTts,
                "Mistral Voxtral 4B TTS",
            )
            .changed();
    });
    changed
}

// ---------------------------------------------------------------------------
// Open-weights provider control panels
//
// Kept intentionally minimal — base URL, API key, voice/reference, speed.
// The Windows GUI surfaces only what the worker actually consumes; users
// configure deeper knobs (temperature, top_p, style prompt) on the Settings
// dialog or via config.toml.
// ---------------------------------------------------------------------------

fn provider_speed_slider(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut f32,
    min: f32,
    max: f32,
) -> bool {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(egui::Slider::new(value, min..=max).step_by(0.05))
    })
    .inner
    .changed()
}

fn render_deferred_notice(ui: &mut egui::Ui, title: &str) {
    ui.label(egui::RichText::new(title).strong());
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new("Offline voice generation is not available for this model yet.")
            .color(egui::Color32::from_rgb(255, 165, 0)),
    );
}

fn render_step_audio_controls(ui: &mut egui::Ui, config: &mut Config) -> bool {
    let mut changed = false;
    ui.label(egui::RichText::new("Step Audio EditX").strong());
    ui.label(
        egui::RichText::new(
            "Supports Mandarin, English, Sichuanese, Cantonese, Japanese, and Korean.",
        )
        .color(egui::Color32::from_rgb(96, 125, 139)),
    );
    changed |= reference_library::render_reference_voice_selector(
        ui,
        &config.step_audio_reference_voices,
        &mut config.tts_playground.step_audio_settings,
        "tts_playground_step_audio_reference",
    );
    changed
}

fn render_step_audio_edit_controls(
    ui: &mut egui::Ui,
    config: &mut Config,
    state: &mut TtsPlaygroundUiState,
) -> bool {
    let mut changed = false;
    let settings = &mut config.tts_playground.step_audio_edit_settings;
    ui.label(egui::RichText::new("Step Audio EditX Audio Edit").strong());
    ui.horizontal_wrapped(|ui| {
        if ui.button("Pick source audio").clicked() {
            if let Ok(Some(path)) = export::pick_audio_file_dialog() {
                settings.source_audio_path = path.display().to_string();
                changed = true;
            }
        }
        if ui.button("Use current clip").clicked()
            && let Some(current) = &state.current
            && let Ok(path) = library::save_managed_wav("edit-source-current", &current.wav_data)
        {
            settings.source_audio_path = path.display().to_string();
            if settings.source_text.trim().is_empty() {
                settings.source_text = current.text.clone();
            }
            changed = true;
        }
        if state.mic_recording.is_some() {
            if ui.button("Stop mic").clicked() {
                if let Some(recording) = state.mic_recording.take() {
                    recording
                        .stop
                        .store(true, std::sync::atomic::Ordering::Relaxed);
                    drop(recording.stream);
                    if let Ok(samples) = recording.samples.lock()
                        && let Ok(path) =
                            library::encode_managed_wav("edit-source-mic", &samples, 16_000)
                    {
                        settings.source_audio_path = path.display().to_string();
                        changed = true;
                    }
                }
            }
        } else if ui.button("Record mic").clicked() {
            let samples = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
            let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let pause = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            if let Ok(stream) =
                crate::api::realtime_audio::start_mic_capture(samples.clone(), stop.clone(), pause)
            {
                state.mic_recording = Some(MicRecordingState {
                    samples,
                    stop,
                    stream,
                });
            }
        }
    });
    ui.label(
        egui::RichText::new(if settings.source_audio_path.trim().is_empty() {
            "No source audio selected"
        } else {
            settings.source_audio_path.as_str()
        })
        .small()
        .color(egui::Color32::from_rgb(96, 125, 139)),
    );
    ui.label("Exact source transcript:");
    changed |= ui
        .add(
            egui::TextEdit::multiline(&mut settings.source_text)
                .desired_rows(4)
                .desired_width(f32::INFINITY),
        )
        .changed();
    ui.horizontal(|ui| {
        ui.label("Task:");
        egui::ComboBox::from_id_salt("tts_playground_step_audio_edit_type")
            .selected_text(&settings.edit_type)
            .show_ui(ui, |ui| {
                for task in [
                    "emotion",
                    "style",
                    "speed",
                    "denoise",
                    "vad",
                    "paralinguistic",
                ] {
                    changed |= ui
                        .selectable_value(&mut settings.edit_type, task.to_string(), task)
                        .changed();
                }
            });
    });
    let options = step_audio_edit_info_options(&settings.edit_type);
    if !options.is_empty() {
        ui.horizontal(|ui| {
            ui.label("Sub-task:");
            egui::ComboBox::from_id_salt("tts_playground_step_audio_edit_info")
                .selected_text(if settings.edit_info.trim().is_empty() {
                    options[0]
                } else {
                    &settings.edit_info
                })
                .show_ui(ui, |ui| {
                    for option in options {
                        changed |= ui
                            .selectable_value(&mut settings.edit_info, option.to_string(), *option)
                            .changed();
                    }
                });
        });
    }
    if settings.edit_type == "paralinguistic" {
        ui.horizontal_wrapped(|ui| {
            ui.label("Inline sound tag:");
            egui::ComboBox::from_id_salt("tts_playground_step_audio_paralinguistic_tag")
                .selected_text("Insert tag")
                .width(130.0)
                .show_ui(ui, |ui| {
                    for tag in reference_library::STEP_AUDIO_PARALINGUISTIC_TAGS {
                        if ui.selectable_label(false, *tag).clicked() {
                            append_inline_tag(&mut settings.target_text, tag);
                            changed = true;
                        }
                    }
                });
        });
        ui.label("Target text:");
        changed |= ui
            .add(
                egui::TextEdit::multiline(&mut settings.target_text)
                    .desired_rows(3)
                    .desired_width(f32::INFINITY),
            )
            .changed();
    }
    changed
}

fn append_inline_tag(text: &mut String, tag: &str) {
    if !text.is_empty() && !text.ends_with(char::is_whitespace) {
        text.push(' ');
    }
    text.push_str(tag);
    text.push(' ');
}

fn step_audio_edit_info_options(edit_type: &str) -> &'static [&'static str] {
    match edit_type {
        "emotion" => &[
            "happy",
            "angry",
            "sad",
            "humour",
            "confusion",
            "disgusted",
            "empathy",
            "embarrass",
            "fear",
            "surprised",
            "excited",
            "depressed",
            "coldness",
            "admiration",
            "remove",
        ],
        "style" => &[
            "serious",
            "arrogant",
            "child",
            "older",
            "girl",
            "pure",
            "sister",
            "sweet",
            "ethereal",
            "whisper",
            "gentle",
            "recite",
            "generous",
            "act_coy",
            "warm",
            "shy",
            "comfort",
            "authority",
            "chat",
            "radio",
            "soulful",
            "story",
            "vivid",
            "program",
            "news",
            "advertising",
            "roar",
            "murmur",
            "shout",
            "deeply",
            "loudly",
            "remove",
            "exaggerated",
        ],
        "speed" => &["faster", "slower", "more faster", "more slower"],
        _ => &[],
    }
}

fn render_magpie_controls(ui: &mut egui::Ui, config: &mut Config) -> bool {
    let mut changed = false;
    ui.label(egui::RichText::new("NVIDIA Magpie-Multilingual 357M").strong());
    ui.label(
        egui::RichText::new(
            "Supports English, Spanish, German, French, Vietnamese, Italian, Mandarin Chinese, Hindi, and Japanese.",
        )
        .color(egui::Color32::from_rgb(96, 125, 139)),
    );
    changed |= render_magpie_voice_config_rows(ui, config);
    changed
}

fn render_magpie_voice_config_rows(ui: &mut egui::Ui, config: &mut Config) -> bool {
    let mut changed = false;
    ui.add_space(10.0);
    ui.separator();
    ui.add_space(6.0);
    ui.label(egui::RichText::new("Voice per Language:").strong());

    egui::ScrollArea::vertical()
        .max_height(180.0)
        .show(ui, |ui| {
            let mut to_remove: Option<usize> = None;
            for (idx, voice_config) in config
                .tts_playground
                .magpie_settings
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
                    egui::ComboBox::from_id_salt(format!("tts_playground_magpie_voice_{idx}"))
                        .selected_text(normalize_magpie_voice(&voice_config.voice_id))
                        .width(240.0)
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

                    if icon_button(ui, Icon::Close).clicked() {
                        to_remove = Some(idx);
                    }
                });
            }

            if let Some(idx) = to_remove {
                config
                    .tts_playground
                    .magpie_settings
                    .voice_configs
                    .remove(idx);
                changed = true;
            }
        });

    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let used_codes: Vec<_> = config
            .tts_playground
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
            egui::ComboBox::from_id_salt("tts_playground_magpie_add_language")
                .selected_text("Add Voice Config")
                .width(160.0)
                .show_ui(ui, |ui| {
                    for (code, name) in &available {
                        if ui.selectable_label(false, *name).clicked() {
                            config.tts_playground.magpie_settings.voice_configs.push(
                                MagpieVoiceConfig::new(
                                    code,
                                    name,
                                    default_magpie_voice_for_lang(code),
                                ),
                            );
                            changed = true;
                        }
                    }
                });
        }

        if ui.button("Reset to Defaults").clicked() {
            config.tts_playground.magpie_settings.voice_configs =
                crate::config::MagpieSettings::default().voice_configs;
            changed = true;
        }
    });

    changed
}

fn render_kokoro_controls(ui: &mut egui::Ui, config: &mut Config, text: &LocaleText) -> bool {
    let mut changed = false;
    let s = &mut config.tts_playground.kokoro_settings;
    ui.label(egui::RichText::new(text.tts_kokoro_title).strong());
    ui.label(
        egui::RichText::new(text.tts_kokoro_desc).color(egui::Color32::from_rgb(96, 125, 139)),
    );
    changed |= provider_speed_slider(ui, text.tts_speed_label, &mut s.speed, 0.5, 2.0);
    ui.horizontal(|ui| {
        ui.label(text.tts_cpu_threads_label);
        changed |= ui
            .add(egui::Slider::new(&mut s.num_threads, 1..=8))
            .changed();
    });
    changed |= render_kokoro_voice_config_rows(ui, config, text);
    changed
}

fn render_kokoro_voice_config_rows(
    ui: &mut egui::Ui,
    config: &mut Config,
    text: &LocaleText,
) -> bool {
    let mut changed = false;
    ui.add_space(10.0);
    ui.separator();
    ui.add_space(6.0);
    ui.label(egui::RichText::new(text.tts_voice_per_language_label).strong());

    egui::ScrollArea::vertical()
        .max_height(180.0)
        .show(ui, |ui| {
            let mut to_remove: Option<usize> = None;
            for (idx, voice_config) in config
                .tts_playground
                .kokoro_settings
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

                    let voice_lang =
                        kokoro_voice_language_for_condition(&voice_config.language_code)
                            .unwrap_or("en-us");
                    egui::ComboBox::from_id_salt(format!("tts_playground_kokoro_voice_{idx}"))
                        .selected_text(&voice_config.voice_id)
                        .width(240.0)
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

                    if icon_button(ui, Icon::Close).clicked() {
                        to_remove = Some(idx);
                    }
                });
            }

            if let Some(idx) = to_remove {
                config
                    .tts_playground
                    .kokoro_settings
                    .voice_configs
                    .remove(idx);
                changed = true;
            }
        });

    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let used_codes: Vec<_> = config
            .tts_playground
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
            egui::ComboBox::from_id_salt("tts_playground_kokoro_add_language")
                .selected_text(text.tts_add_language_label)
                .width(160.0)
                .show_ui(ui, |ui| {
                    for (code, name) in &available {
                        if ui.selectable_label(false, *name).clicked() {
                            let voice_lang =
                                kokoro_voice_language_for_condition(code).unwrap_or("en-us");
                            config.tts_playground.kokoro_settings.voice_configs.push(
                                KokoroVoiceConfig {
                                    language_code: (*code).to_string(),
                                    language_name: (*name).to_string(),
                                    voice_id: default_kokoro_voice_for_lang(voice_lang).to_string(),
                                },
                            );
                            changed = true;
                        }
                    }
                });
        }

        if ui.button(text.tts_reset_to_defaults_label).clicked() {
            config.tts_playground.kokoro_settings.voice_configs =
                crate::config::KokoroSettings::default().voice_configs;
            changed = true;
        }
    });

    changed
}

fn render_supertonic_controls(ui: &mut egui::Ui, config: &mut Config, text: &LocaleText) -> bool {
    let mut changed = false;
    let s = &mut config.tts_playground.supertonic_settings;
    ui.label(egui::RichText::new("Supertonic 3").strong());
    ui.label(
        egui::RichText::new(SUPERTONIC_LANGUAGE_SUMMARY)
            .color(egui::Color32::from_rgb(96, 125, 139)),
    );
    changed |= provider_speed_slider(ui, text.tts_speed_label, &mut s.speed, 0.5, 2.0);
    ui.horizontal(|ui| {
        ui.label(text.tts_cpu_threads_label);
        changed |= ui
            .add(egui::Slider::new(&mut s.num_threads, 1..=8))
            .changed();
    });
    ui.horizontal(|ui| {
        ui.label("Quality steps");
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
    ui.add_space(10.0);
    ui.separator();
    ui.add_space(6.0);
    ui.label(egui::RichText::new(text.tts_voice_per_language_label).strong());

    egui::ScrollArea::vertical()
        .max_height(180.0)
        .show(ui, |ui| {
            let mut to_remove: Option<usize> = None;
            for (idx, voice_config) in config
                .tts_playground
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
                    egui::ComboBox::from_id_salt(format!("tts_playground_supertonic_voice_{idx}"))
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
                    if icon_button(ui, Icon::Close).clicked() {
                        to_remove = Some(idx);
                    }
                });
            }
            if let Some(idx) = to_remove {
                config
                    .tts_playground
                    .supertonic_settings
                    .voice_configs
                    .remove(idx);
                changed = true;
            }
        });

    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let used_codes: Vec<_> = config
            .tts_playground
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
            egui::ComboBox::from_id_salt("tts_playground_supertonic_add_language")
                .selected_text(text.tts_add_language_label)
                .width(160.0)
                .show_ui(ui, |ui| {
                    for lang in &available {
                        if ui.selectable_label(false, lang.label).clicked() {
                            config
                                .tts_playground
                                .supertonic_settings
                                .voice_configs
                                .push(SupertonicVoiceConfig::new(
                                    lang.code,
                                    lang.label,
                                    default_supertonic_voice_for_lang(lang.code),
                                ));
                            changed = true;
                        }
                    }
                });
        }

        if ui.button(text.tts_reset_to_defaults_label).clicked() {
            config.tts_playground.supertonic_settings.voice_configs =
                crate::config::SupertonicSettings::default().voice_configs;
            changed = true;
        }
    });

    changed
}

fn render_voxtral_controls(ui: &mut egui::Ui, _config: &mut Config) -> bool {
    render_deferred_notice(ui, "Mistral Voxtral 4B TTS (deferred)");
    false
}

fn render_vieneu_controls(ui: &mut egui::Ui, config: &mut Config) -> bool {
    let mut changed = false;
    ui.label(egui::RichText::new("VieNeu-TTS v2").strong());
    ui.label(
        egui::RichText::new("Vietnamese-first local TTS with English/Vietnamese code-switching and zero-shot voice cloning.")
            .color(egui::Color32::from_rgb(96, 125, 139)),
    );
    ui.label(
        egui::RichText::new(
            "Uses the verified VieNeu-TTS-v2 Turbo GPU path. Reference voice is the only supported user control.",
        )
            .small()
            .color(egui::Color32::from_rgb(96, 125, 139)),
    );
    changed |= render_vieneu_reference_selector(ui, config);
    changed
}

fn render_vieneu_reference_selector(ui: &mut egui::Ui, config: &mut Config) -> bool {
    let mut changed = false;
    let settings = &mut config.tts_playground.vieneu_settings;
    ui.horizontal(|ui| {
        ui.label("Reference voice:");
        let selected = if settings.reference_voice_id.trim().is_empty() {
            "Model default voice".to_string()
        } else {
            config
                .step_audio_reference_voices
                .iter()
                .find(|reference| reference.id == settings.reference_voice_id)
                .map(reference_library::reference_label)
                .unwrap_or_else(|| "Missing reference".to_string())
        };
        egui::ComboBox::from_id_salt("tts_playground_vieneu_reference")
            .selected_text(selected)
            .width(220.0)
            .show_ui(ui, |ui| {
                changed |= ui
                    .selectable_value(
                        &mut settings.reference_voice_id,
                        String::new(),
                        "Model default voice",
                    )
                    .changed();
                for reference in &config.step_audio_reference_voices {
                    changed |= ui
                        .selectable_value(
                            &mut settings.reference_voice_id,
                            reference.id.clone(),
                            reference_library::reference_label(reference),
                        )
                        .changed();
                }
            });
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
