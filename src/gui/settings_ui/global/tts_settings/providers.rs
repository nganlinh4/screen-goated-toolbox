use crate::config::tts_catalog::{
    KOKORO_VOICE_LANGUAGES, KOKORO_VOICES, MAGPIE_VOICE_LANGUAGES, MAGPIE_VOICES,
    SUPERTONIC_LANGUAGE_SUMMARY, SUPERTONIC_LANGUAGES, SUPERTONIC_VOICES,
    default_kokoro_voice_for_lang, default_magpie_voice_for_lang,
    default_supertonic_voice_for_lang, kokoro_voice_language_for_condition, normalize_magpie_voice,
    normalize_supertonic_lang, normalize_supertonic_voice,
};
use crate::config::{Config, KokoroVoiceConfig, MagpieVoiceConfig, SupertonicVoiceConfig};
use crate::gui::icons::{Icon, icon_button};
use crate::gui::locale::LocaleText;
use crate::gui::theme::AppTheme;
use eframe::egui;

use super::speak_settings_preview;

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

pub(super) fn render_step_audio_settings(
    ui: &mut egui::Ui,
    config: &mut Config,
    text: &LocaleText,
) -> bool {
    let mut changed = false;
    render_open_weights_header(
        ui,
        "Step Audio EditX",
        text.tts_advanced.tts_step_audio_desc,
    );
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
        ui.label(text.tts_advanced.tts_reference_voice_label);
        let selected = config
            .step_audio_reference_voices
            .iter()
            .find(|item| item.id == config.step_audio_settings.reference_voice_id)
            .map(|item| reference_label_or_default(item, text))
            .unwrap_or_else(|| text.tts_advanced.tts_reference_default.to_string());
        crate::gui::widgets::combo("step_audio_global_reference_voice")
            .selected_text(selected)
            .width(240.0)
            .show_ui(ui, |ui| {
                changed |= ui
                    .selectable_value(
                        &mut config.step_audio_settings.reference_voice_id,
                        String::new(),
                        text.tts_advanced.tts_reference_default,
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

pub(super) fn render_magpie_settings(
    ui: &mut egui::Ui,
    config: &mut Config,
    text: &LocaleText,
) -> bool {
    let mut changed = false;
    render_open_weights_header(
        ui,
        "NVIDIA Magpie-Multilingual 357M",
        text.auxiliary.managed_tools.tool_desc_magpie,
    );
    changed |= render_magpie_voice_config_rows(ui, config, text);
    changed
}

fn render_magpie_voice_config_rows(
    ui: &mut egui::Ui,
    config: &mut Config,
    text: &LocaleText,
) -> bool {
    let theme = AppTheme::from_ui(ui);
    let mut changed = false;
    ui.add_space(15.0);
    ui.separator();
    ui.add_space(10.0);
    ui.label(egui::RichText::new(text.tts_settings.tts_voice_per_language_label).strong());
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
                            .color(theme.success()),
                    );
                    crate::gui::icons::draw_icon_static(
                        ui,
                        crate::gui::icons::Icon::ArrowRightAlt,
                        Some(crate::gui::icons::ICON_SM),
                    );
                    crate::gui::widgets::combo(format!("magpie_voice_{}", idx))
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
                        .on_hover_text(text.tts_advanced.remove_label)
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
            crate::gui::widgets::combo("magpie_add_language")
                .selected_text(text.tts_settings.tts_add_language_label)
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

        if ui
            .button(text.tts_settings.tts_reset_to_defaults_label)
            .clicked()
        {
            config.magpie_settings.voice_configs =
                crate::config::MagpieSettings::default().voice_configs;
            changed = true;
        }
    });
    changed
}

pub(super) fn render_kokoro_settings(
    ui: &mut egui::Ui,
    config: &mut Config,
    text: &LocaleText,
) -> bool {
    let mut changed = false;
    render_open_weights_header(
        ui,
        text.tts_advanced.tts_kokoro_title,
        text.tts_advanced.tts_kokoro_desc,
    );
    let s = &mut config.kokoro_settings;
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(text.tts_advanced.tts_cpu_threads_label).strong());
        changed |= ui
            .add(egui::Slider::new(&mut s.num_threads, 1..=8))
            .changed();
    });
    changed |= render_speed_row(
        ui,
        text.tts_settings.tts_speed_label,
        &mut s.speed,
        0.5,
        2.0,
    );
    changed |= render_kokoro_voice_config_rows(ui, config, text);
    changed
}

fn render_kokoro_voice_config_rows(
    ui: &mut egui::Ui,
    config: &mut Config,
    text: &LocaleText,
) -> bool {
    let theme = AppTheme::from_ui(ui);
    let mut changed = false;
    ui.add_space(15.0);
    ui.separator();
    ui.add_space(10.0);
    ui.label(egui::RichText::new(text.tts_settings.tts_voice_per_language_label).strong());
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
                            .color(theme.success()),
                    );
                    crate::gui::icons::draw_icon_static(
                        ui,
                        crate::gui::icons::Icon::ArrowRightAlt,
                        Some(crate::gui::icons::ICON_SM),
                    );

                    let voice_lang =
                        kokoro_voice_language_for_condition(&voice_config.language_code)
                            .unwrap_or("en-us");
                    crate::gui::widgets::combo(format!("kokoro_voice_{idx}"))
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
                        .on_hover_text(text.tts_settings.tts_preview_label)
                        .clicked()
                    {
                        speak_settings_preview(text, &voice_config.voice_id);
                    }
                    if icon_button(ui, Icon::Close)
                        .on_hover_text(text.tts_advanced.remove_label)
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
            crate::gui::widgets::combo("kokoro_add_language")
                .selected_text(text.tts_settings.tts_add_language_label)
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

        if ui
            .button(text.tts_settings.tts_reset_to_defaults_label)
            .clicked()
        {
            config.kokoro_settings.voice_configs =
                crate::config::KokoroSettings::default().voice_configs;
            changed = true;
        }
    });

    changed
}

pub(super) fn render_supertonic_settings(
    ui: &mut egui::Ui,
    config: &mut Config,
    text: &LocaleText,
) -> bool {
    let mut changed = false;
    render_open_weights_header(ui, "Supertonic 3", SUPERTONIC_LANGUAGE_SUMMARY);
    let s = &mut config.supertonic_settings;
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(text.tts_advanced.tts_cpu_threads_label).strong());
        changed |= ui
            .add(egui::Slider::new(&mut s.num_threads, 1..=8))
            .changed();
    });
    changed |= render_speed_row(
        ui,
        text.tts_settings.tts_speed_label,
        &mut s.speed,
        0.5,
        2.0,
    );
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(text.tts_advanced.tts_quality_steps_label).strong());
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
    let theme = AppTheme::from_ui(ui);
    let mut changed = false;
    ui.add_space(15.0);
    ui.separator();
    ui.add_space(10.0);
    ui.label(egui::RichText::new(text.tts_settings.tts_voice_per_language_label).strong());

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
                            .color(theme.success()),
                    );
                    crate::gui::icons::draw_icon_static(
                        ui,
                        crate::gui::icons::Icon::ArrowRightAlt,
                        Some(crate::gui::icons::ICON_SM),
                    );
                    crate::gui::widgets::combo(format!("supertonic_voice_{idx}"))
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
                        .on_hover_text(text.tts_advanced.remove_label)
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
            crate::gui::widgets::combo("supertonic_add_language")
                .selected_text(text.tts_settings.tts_add_language_label)
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

        if ui
            .button(text.tts_settings.tts_reset_to_defaults_label)
            .clicked()
        {
            config.supertonic_settings.voice_configs =
                crate::config::SupertonicSettings::default().voice_configs;
            changed = true;
        }
    });

    changed
}

pub(super) fn render_vieneu_settings(
    ui: &mut egui::Ui,
    config: &mut Config,
    text: &LocaleText,
) -> bool {
    let theme = AppTheme::from_ui(ui);
    let mut changed = false;
    render_open_weights_header(ui, "VieNeu-TTS v2", text.tts_advanced.tts_vieneu_desc);
    ui.label(
        egui::RichText::new(text.tts_advanced.tts_vieneu_control_desc)
            .small()
            .color(theme.on_surface_variant()),
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
        ui.label(text.tts_advanced.tts_reference_voice_label);
        let selected = config
            .step_audio_reference_voices
            .iter()
            .find(|item| item.id == config.vieneu_settings.reference_voice_id)
            .map(|item| reference_label_or_default(item, text))
            .unwrap_or_else(|| text.tts_advanced.tts_reference_default.to_string());
        crate::gui::widgets::combo("vieneu_global_reference_voice")
            .selected_text(selected)
            .width(240.0)
            .show_ui(ui, |ui| {
                changed |= ui
                    .selectable_value(
                        &mut config.vieneu_settings.reference_voice_id,
                        String::new(),
                        text.tts_advanced.tts_reference_default,
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
        text.tts_advanced.tts_reference_untitled.to_string()
    } else {
        reference.label.clone()
    }
}
