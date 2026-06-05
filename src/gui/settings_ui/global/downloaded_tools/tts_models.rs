use crate::api::realtime_audio::magpie_assets::{
    current_magpie_notice, download_magpie_model, get_magpie_model_dir, is_magpie_model_downloaded,
    remove_magpie_model,
};
use crate::api::realtime_audio::magpie_runtime::{
    current_magpie_runtime_notice, download_magpie_runtime, is_magpie_runtime_downloading,
    is_magpie_runtime_installed, magpie_runtime_installed_size, remove_magpie_runtime,
};
use crate::api::realtime_audio::step_audio_assets::{
    current_step_audio_notice, download_step_audio_model, get_step_audio_model_dir,
    is_step_audio_model_downloaded, remove_step_audio_model,
};
use crate::api::realtime_audio::step_audio_runtime::{
    current_step_audio_runtime_notice, download_step_audio_runtime,
    is_step_audio_runtime_downloading, is_step_audio_runtime_installed, remove_step_audio_runtime,
    step_audio_runtime_installed_size,
};
use crate::api::realtime_audio::vieneu_runtime::{
    current_vieneu_runtime_notice, download_vieneu_runtime, is_vieneu_runtime_downloading,
    is_vieneu_runtime_installed_for_variant, remove_vieneu_runtime, vieneu_runtime_installed_size,
};
use crate::gui::locale::LocaleText;
use crate::gui::theme::AppTheme;
use crate::overlay::realtime_webview::state::REALTIME_STATE;
use eframe::egui;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::thread;

use super::utils::{
    cached_probe, cached_u64, format_size, get_dir_size, invalidate_probe_cache,
    invalidate_size_cache, invalidate_u64_cache, tool_card,
};

const PROBE_STEP_AUDIO: &str = "downloaded-tools:step-audio-editx";
const PROBE_MAGPIE: &str = "downloaded-tools:magpie-multilingual";
const PROBE_STEP_RUNTIME: &str = "downloaded-tools:step-audio-runtime";
const PROBE_MAGPIE_RUNTIME: &str = "downloaded-tools:magpie-runtime";
const PROBE_VIENEU_RUNTIME: &str = "downloaded-tools:vieneu-runtime";

struct TtsCardSpec {
    title: &'static str,
    model_probe: &'static str,
    description: &'static str,
    model_title: &'static str,
    model_download_title: &'static str,
    model_notice: fn() -> Option<String>,
    is_model_downloaded: fn() -> bool,
    model_dir: fn() -> PathBuf,
    download_model: fn(Arc<AtomicBool>, bool) -> anyhow::Result<()>,
    remove_model: fn() -> anyhow::Result<()>,
}

pub(super) fn render_step_audio_card(ui: &mut egui::Ui, text: &LocaleText) {
    tool_card(ui, |ui| {
        let spec = TtsCardSpec {
            title: "Step Audio EditX",
            model_probe: PROBE_STEP_AUDIO,
            description: text.tool_desc_step_audio,
            model_title: "Step Audio AWQ model + tokenizer",
            model_download_title: text.step_audio_downloading_title,
            model_notice: current_step_audio_notice,
            is_model_downloaded: is_step_audio_model_downloaded,
            model_dir: get_step_audio_model_dir,
            download_model: download_step_audio_model,
            remove_model: remove_step_audio_model,
        };
        ui.heading(spec.title);
        ui.add_space(4.0);
        ui.label(spec.description);
        ui.add_space(6.0);
        render_model_row(ui, text, &spec);
        ui.add_space(4.0);
        render_step_audio_runtime_row(ui, text);
    });
}

pub(super) fn render_magpie_card(ui: &mut egui::Ui, text: &LocaleText) {
    tool_card(ui, |ui| {
        let spec = TtsCardSpec {
            title: "NVIDIA Magpie-Multilingual 357M",
            model_probe: PROBE_MAGPIE,
            description: text.tool_desc_magpie,
            model_title: "Magpie model + NanoCodec",
            model_download_title: text.magpie_downloading_title,
            model_notice: current_magpie_notice,
            is_model_downloaded: is_magpie_model_downloaded,
            model_dir: get_magpie_model_dir,
            download_model: download_magpie_model,
            remove_model: remove_magpie_model,
        };
        ui.heading(spec.title);
        ui.add_space(4.0);
        ui.label(spec.description);
        ui.add_space(6.0);
        render_model_row(ui, text, &spec);
        ui.add_space(4.0);
        render_magpie_runtime_row(ui, text);
    });
}

fn render_step_audio_runtime_row(ui: &mut egui::Ui, text: &LocaleText) {
    let theme = AppTheme::from_ui(ui);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(text.tool_step_audio_runtime).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let is_downloading = is_step_audio_runtime_downloading()
                || REALTIME_STATE
                    .lock()
                    .map(|s| {
                        s.is_downloading && s.download_title == "Downloading Step Audio runtime"
                    })
                    .unwrap_or(false);
            if is_downloading {
                let progress = REALTIME_STATE
                    .lock()
                    .map(|s| s.download_progress)
                    .unwrap_or(0.0);
                ui.label(format!("{progress:.0}%"));
                ui.spinner();
            } else if cached_probe(PROBE_STEP_RUNTIME, is_step_audio_runtime_installed) {
                if ui
                    .button(egui::RichText::new(text.tool_action_delete).color(theme.danger_text()))
                    .clicked()
                {
                    invalidate_probe_cache(PROBE_STEP_RUNTIME);
                    invalidate_u64_cache("downloaded-tools:step-audio-runtime-size");
                    let _ = remove_step_audio_runtime();
                }
                let size = cached_u64(
                    "downloaded-tools:step-audio-runtime-size",
                    step_audio_runtime_installed_size,
                );
                ui.label(
                    egui::RichText::new(
                        text.tool_status_installed.replace("{}", &format_size(size)),
                    )
                    .color(theme.success()),
                );
            } else {
                if ui.button(text.tool_action_download).clicked() {
                    let stop_signal = Arc::new(AtomicBool::new(false));
                    thread::spawn(move || {
                        let _ = download_step_audio_runtime(stop_signal, false);
                    });
                }
                ui.label(egui::RichText::new(text.tool_status_missing).color(egui::Color32::GRAY));
            }
        });
    });
    ui.label(text.tool_desc_step_audio_runtime);
    if let Some(message) = current_step_audio_runtime_notice() {
        ui.label(egui::RichText::new(message).color(theme.danger_text()));
    }
}

pub(super) fn render_vieneu_card(ui: &mut egui::Ui, text: &LocaleText) {
    tool_card(ui, |ui| {
        let theme = AppTheme::from_ui(ui);
        ui.heading("VieNeu-TTS v2");
        ui.add_space(4.0);
        ui.label(text.tool_desc_vieneu);
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(text.tool_vieneu_runtime).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let is_downloading = is_vieneu_runtime_downloading()
                    || REALTIME_STATE
                        .lock()
                        .map(|s| {
                            s.is_downloading
                                && s.download_title == text.vieneu_runtime_downloading_title
                        })
                        .unwrap_or(false);
                let probe_variant = crate::config::tts_catalog::default_vieneu_variant_id();
                if is_downloading {
                    let progress = REALTIME_STATE
                        .lock()
                        .map(|s| s.download_progress)
                        .unwrap_or(0.0);
                    ui.label(format!("{progress:.0}%"));
                    ui.spinner();
                } else if cached_probe(PROBE_VIENEU_RUNTIME, move || {
                    is_vieneu_runtime_installed_for_variant(probe_variant)
                }) {
                    if ui
                        .button(
                            egui::RichText::new(text.tool_action_delete).color(theme.danger_text()),
                        )
                        .clicked()
                    {
                        invalidate_probe_cache(PROBE_VIENEU_RUNTIME);
                        invalidate_u64_cache("downloaded-tools:vieneu-runtime-size");
                        let _ = remove_vieneu_runtime();
                    }
                    let size = cached_u64(
                        "downloaded-tools:vieneu-runtime-size",
                        vieneu_runtime_installed_size,
                    );
                    ui.label(
                        egui::RichText::new(
                            text.tool_status_installed.replace("{}", &format_size(size)),
                        )
                        .color(theme.success()),
                    );
                } else {
                    if ui.button(text.tool_action_download).clicked() {
                        let stop_signal = Arc::new(AtomicBool::new(false));
                        let install_variant =
                            crate::config::tts_catalog::default_vieneu_variant_id().to_string();
                        thread::spawn(move || {
                            let _ = download_vieneu_runtime(stop_signal, false, install_variant);
                        });
                    }
                    ui.label(
                        egui::RichText::new(text.tool_status_missing).color(egui::Color32::GRAY),
                    );
                }
            });
        });
        if let Some(notice) = current_vieneu_runtime_notice() {
            ui.label(egui::RichText::new(notice).color(theme.warning()));
        }
        ui.label(text.tool_desc_vieneu_runtime);
    });
}

fn render_model_row(ui: &mut egui::Ui, text: &LocaleText, spec: &TtsCardSpec) {
    let theme = AppTheme::from_ui(ui);
    let notice = (spec.model_notice)();
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(spec.model_title).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let is_downloading = REALTIME_STATE
                .lock()
                .map(|s| s.is_downloading && s.download_title == spec.model_download_title)
                .unwrap_or(false);
            if is_downloading {
                let progress = REALTIME_STATE
                    .lock()
                    .map(|s| s.download_progress)
                    .unwrap_or(0.0);
                ui.label(format!("{progress:.0}%"));
                ui.spinner();
            } else if cached_probe(spec.model_probe, spec.is_model_downloaded) {
                if ui
                    .button(egui::RichText::new(text.tool_action_delete).color(theme.danger_text()))
                    .clicked()
                {
                    invalidate_size_cache(&(spec.model_dir)());
                    invalidate_probe_cache(spec.model_probe);
                    let _ = (spec.remove_model)();
                }
                let size = get_dir_size(&(spec.model_dir)());
                ui.label(
                    egui::RichText::new(
                        text.tool_status_installed.replace("{}", &format_size(size)),
                    )
                    .color(theme.success()),
                );
            } else {
                if ui.button(text.tool_action_download).clicked() {
                    let stop_signal = Arc::new(AtomicBool::new(false));
                    let download_model = spec.download_model;
                    thread::spawn(move || {
                        let _ = download_model(stop_signal, false);
                    });
                }
                ui.label(egui::RichText::new(text.tool_status_missing).color(egui::Color32::GRAY));
            }
        });
    });
    if let Some(message) = notice {
        ui.label(egui::RichText::new(message).color(theme.danger_text()));
    }
}

fn render_magpie_runtime_row(ui: &mut egui::Ui, text: &LocaleText) {
    let theme = AppTheme::from_ui(ui);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(text.tool_magpie_runtime).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let is_downloading = is_magpie_runtime_downloading()
                || REALTIME_STATE
                    .lock()
                    .map(|s| s.is_downloading && s.download_title == "Downloading Magpie runtime")
                    .unwrap_or(false);
            if is_downloading {
                let progress = REALTIME_STATE
                    .lock()
                    .map(|s| s.download_progress)
                    .unwrap_or(0.0);
                ui.label(format!("{progress:.0}%"));
                ui.spinner();
            } else if cached_probe(PROBE_MAGPIE_RUNTIME, is_magpie_runtime_installed) {
                if ui
                    .button(egui::RichText::new(text.tool_action_delete).color(theme.danger_text()))
                    .clicked()
                {
                    invalidate_probe_cache(PROBE_MAGPIE_RUNTIME);
                    invalidate_u64_cache("downloaded-tools:magpie-runtime-size");
                    let _ = remove_magpie_runtime();
                }
                let size = cached_u64(
                    "downloaded-tools:magpie-runtime-size",
                    magpie_runtime_installed_size,
                );
                ui.label(
                    egui::RichText::new(
                        text.tool_status_installed.replace("{}", &format_size(size)),
                    )
                    .color(theme.success()),
                );
            } else {
                if ui.button(text.tool_action_download).clicked() {
                    let stop_signal = Arc::new(AtomicBool::new(false));
                    thread::spawn(move || {
                        let _ = download_magpie_runtime(stop_signal, false);
                    });
                }
                ui.label(egui::RichText::new(text.tool_status_missing).color(egui::Color32::GRAY));
            }
        });
    });
    ui.label(text.tool_desc_magpie_runtime);
    if let Some(message) = current_magpie_runtime_notice() {
        ui.label(egui::RichText::new(message).color(theme.danger_text()));
    }
}
