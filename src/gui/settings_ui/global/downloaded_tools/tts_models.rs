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
use crate::api::realtime_audio::tts_libtorch_runtime_assets::{
    TtsRuntimeSpec, VOXTRAL_RUNTIME, current_tts_runtime_notice, download_tts_runtime,
    is_tts_runtime_installed, remove_tts_runtime, tts_runtime_installed_size,
};
use crate::api::realtime_audio::voxtral_assets::{
    current_voxtral_notice, download_voxtral_model, get_voxtral_model_dir,
    is_voxtral_model_downloaded, remove_voxtral_model,
};
use crate::gui::locale::LocaleText;
use crate::overlay::realtime_webview::state::REALTIME_STATE;
use eframe::egui;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::thread;

use super::utils::{
    cached_probe, cached_u64, format_size, get_dir_size, invalidate_probe_cache,
    invalidate_size_cache, invalidate_u64_cache,
};

const PROBE_STEP_AUDIO: &str = "downloaded-tools:step-audio-editx";
const PROBE_MAGPIE: &str = "downloaded-tools:magpie-multilingual";
const PROBE_VOXTRAL: &str = "downloaded-tools:voxtral-4b";
const PROBE_STEP_RUNTIME: &str = "downloaded-tools:step-audio-runtime";
const PROBE_MAGPIE_RUNTIME: &str = "downloaded-tools:magpie-runtime";
const PROBE_VOXTRAL_RUNTIME: &str = "downloaded-tools:voxtral-runtime";

struct TtsCardSpec {
    title: &'static str,
    model_probe: &'static str,
    runtime_probe: &'static str,
    runtime_size_key: &'static str,
    description: &'static str,
    model_title: &'static str,
    model_download_title: &'static str,
    model_notice: fn() -> Option<String>,
    is_model_downloaded: fn() -> bool,
    model_dir: fn() -> PathBuf,
    download_model: fn(Arc<AtomicBool>, bool) -> anyhow::Result<()>,
    remove_model: fn() -> anyhow::Result<()>,
    runtime: TtsRuntimeSpec,
}

pub(super) fn render_step_audio_card(ui: &mut egui::Ui, text: &LocaleText) {
    ui.group(|ui| {
        let spec = TtsCardSpec {
            title: "Step Audio EditX",
            model_probe: PROBE_STEP_AUDIO,
            runtime_probe: PROBE_STEP_RUNTIME,
            runtime_size_key: "downloaded-tools:step-audio-runtime-size",
            description: text.tool_desc_step_audio,
            model_title: "Step Audio AWQ model + tokenizer",
            model_download_title: text.step_audio_downloading_title,
            model_notice: current_step_audio_notice,
            is_model_downloaded: is_step_audio_model_downloaded,
            model_dir: get_step_audio_model_dir,
            download_model: download_step_audio_model,
            remove_model: remove_step_audio_model,
            runtime: VOXTRAL_RUNTIME,
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
    ui.group(|ui| {
        let spec = TtsCardSpec {
            title: "NVIDIA Magpie-Multilingual 357M",
            model_probe: PROBE_MAGPIE,
            runtime_probe: PROBE_MAGPIE_RUNTIME,
            runtime_size_key: "downloaded-tools:magpie-runtime-size",
            description: text.tool_desc_magpie,
            model_title: "Magpie model + NanoCodec",
            model_download_title: text.magpie_downloading_title,
            model_notice: current_magpie_notice,
            is_model_downloaded: is_magpie_model_downloaded,
            model_dir: get_magpie_model_dir,
            download_model: download_magpie_model,
            remove_model: remove_magpie_model,
            runtime: VOXTRAL_RUNTIME,
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
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Step Audio managed runtime").strong());
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
                    .button(egui::RichText::new(text.tool_action_delete).color(egui::Color32::RED))
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
                    .color(egui::Color32::from_rgb(34, 139, 34)),
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
    ui.label("Python, PyTorch CUDA, official Step-Audio-EditX source, and prompt voices; no system Python or pip required.");
    if let Some(message) = current_step_audio_runtime_notice() {
        ui.label(egui::RichText::new(message).color(egui::Color32::RED));
    }
}

pub(super) fn render_voxtral_card(ui: &mut egui::Ui, text: &LocaleText) {
    render_tts_card(
        ui,
        text,
        TtsCardSpec {
            title: "Mistral Voxtral 4B TTS",
            model_probe: PROBE_VOXTRAL,
            runtime_probe: PROBE_VOXTRAL_RUNTIME,
            runtime_size_key: "downloaded-tools:voxtral-runtime-size",
            description: text.tool_desc_voxtral,
            model_title: "Voxtral weights",
            model_download_title: text.voxtral_downloading_title,
            model_notice: current_voxtral_notice,
            is_model_downloaded: is_voxtral_model_downloaded,
            model_dir: get_voxtral_model_dir,
            download_model: download_voxtral_model,
            remove_model: remove_voxtral_model,
            runtime: VOXTRAL_RUNTIME,
        },
    );
}

fn render_tts_card(ui: &mut egui::Ui, text: &LocaleText, spec: TtsCardSpec) {
    ui.group(|ui| {
        ui.heading(spec.title);
        ui.add_space(4.0);
        ui.label(spec.description);
        ui.add_space(6.0);
        render_model_row(ui, text, &spec);
        ui.add_space(4.0);
        render_runtime_row(ui, text, &spec);
    });
}

fn render_model_row(ui: &mut egui::Ui, text: &LocaleText, spec: &TtsCardSpec) {
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
                    .button(egui::RichText::new(text.tool_action_delete).color(egui::Color32::RED))
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
                    .color(egui::Color32::from_rgb(34, 139, 34)),
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
        ui.label(egui::RichText::new(message).color(egui::Color32::RED));
    }
}

fn render_runtime_row(ui: &mut egui::Ui, text: &LocaleText, spec: &TtsCardSpec) {
    let runtime = spec.runtime;
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(runtime.label).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let is_downloading = REALTIME_STATE
                .lock()
                .map(|s| s.is_downloading && s.download_title == runtime.download_title)
                .unwrap_or(false);
            if is_downloading {
                let progress = REALTIME_STATE
                    .lock()
                    .map(|s| s.download_progress)
                    .unwrap_or(0.0);
                ui.label(format!("{progress:.0}%"));
                ui.spinner();
            } else if cached_probe(spec.runtime_probe, move || {
                is_tts_runtime_installed(runtime)
            }) {
                if ui
                    .button(egui::RichText::new(text.tool_action_delete).color(egui::Color32::RED))
                    .clicked()
                {
                    invalidate_probe_cache(spec.runtime_probe);
                    invalidate_u64_cache(spec.runtime_size_key);
                    let _ = remove_tts_runtime(runtime);
                }
                let size = cached_u64(spec.runtime_size_key, move || {
                    tts_runtime_installed_size(runtime)
                });
                ui.label(
                    egui::RichText::new(
                        text.tool_status_installed.replace("{}", &format_size(size)),
                    )
                    .color(egui::Color32::from_rgb(34, 139, 34)),
                );
            } else {
                if ui.button(text.tool_action_download).clicked() {
                    let stop_signal = Arc::new(AtomicBool::new(false));
                    thread::spawn(move || {
                        let _ = download_tts_runtime(runtime, stop_signal, false);
                    });
                }
                ui.label(egui::RichText::new(text.tool_status_missing).color(egui::Color32::GRAY));
            }
        });
    });
    ui.label("Required support files; downloaded automatically when needed.");
    if let Some(message) = current_tts_runtime_notice(runtime.provider) {
        ui.label(egui::RichText::new(message).color(egui::Color32::RED));
    }
}

fn render_magpie_runtime_row(ui: &mut egui::Ui, text: &LocaleText) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Magpie managed runtime").strong());
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
                    .button(egui::RichText::new(text.tool_action_delete).color(egui::Color32::RED))
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
                    .color(egui::Color32::from_rgb(34, 139, 34)),
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
    ui.label("Python, PyTorch CUDA, and NeMo sidecar; no system Python or pip required.");
    if let Some(message) = current_magpie_runtime_notice() {
        ui.label(egui::RichText::new(message).color(egui::Color32::RED));
    }
}
