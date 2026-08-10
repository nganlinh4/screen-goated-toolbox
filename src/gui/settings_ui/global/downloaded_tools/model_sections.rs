use crate::api::realtime_audio::kokoro_assets::{
    current_kokoro_model_notice, download_kokoro_model, get_kokoro_model_dir,
    is_kokoro_model_downloaded, remove_kokoro_model,
};
use crate::api::realtime_audio::model_loader::{
    current_parakeet_model_notice, download_parakeet_model, get_parakeet_model_dir,
    is_model_downloaded, remove_parakeet_model,
};
use crate::api::realtime_audio::parakeet_tdt_assets::{
    current_parakeet_tdt_model_notice, download_parakeet_tdt_model, get_parakeet_tdt_model_dir,
    is_parakeet_tdt_model_downloaded, remove_parakeet_tdt_model,
};
use crate::api::realtime_audio::qwen3::assets::{
    current_qwen3_model_notice, download_qwen3_1_7b_model, download_qwen3_model,
    get_qwen3_1_7b_model_dir, get_qwen3_model_dir, is_qwen3_1_7b_model_downloaded,
    is_qwen3_model_downloaded, remove_qwen3_1_7b_model, remove_qwen3_model,
};
use crate::api::realtime_audio::supertonic_assets::{
    current_supertonic_model_notice, download_supertonic_model, get_supertonic_model_dir,
    is_supertonic_model_downloaded, remove_supertonic_model,
};
use crate::config::tts_catalog::SUPERTONIC_LANGUAGE_SUMMARY;
use crate::gui::locale::LocaleText;
use crate::gui::theme::AppTheme;
use crate::overlay::realtime_webview::state::REALTIME_STATE;
use eframe::egui;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::thread;

use super::ai_runtime::{
    render_local_asr_worker_content, render_onnx_runtime_content, render_vc_runtime_content,
};
use super::model_card::{ModelRowSpec, render_model_row};
use super::utils::{
    cached_probe, cached_u64, format_size, get_dir_size, get_path_size, invalidate_probe_cache,
    invalidate_size_cache, invalidate_u64_cache, tool_card,
};

const PROBE_PARAKEET_EOU: &str = "downloaded-tools:parakeet-eou";
const PROBE_PARAKEET_TDT: &str = "downloaded-tools:parakeet-tdt";
const PROBE_KOKORO_V1: &str = "downloaded-tools:kokoro-v1";
const PROBE_SUPERTONIC_3: &str = "downloaded-tools:supertonic-3";
const PROBE_QWEN3_SMALL: &str = "downloaded-tools:qwen3-small";
const PROBE_QWEN3_LARGE: &str = "downloaded-tools:qwen3-large";
const PROBE_QWEN3_RUNTIME: &str = "downloaded-tools:qwen3-runtime";
const PROBE_QWEN3_RUNTIME_ACTIVE: &str = "downloaded-tools:qwen3-runtime-active";
const VALUE_QWEN3_RUNTIME_SIZE: &str = "downloaded-tools:qwen3-runtime-size";
const VALUE_QWEN3_RUNTIME_ACTIVE_SIZE: &str = "downloaded-tools:qwen3-runtime-active-size";

pub(super) fn render_parakeet_card(ui: &mut egui::Ui, text: &LocaleText) {
    tool_card(ui, |ui| {
        ui.heading(text.auxiliary.managed_tools.tool_parakeet_card);
        ui.add_space(4.0);
        render_local_asr_worker_content(ui, text);
        ui.add_space(4.0);
        render_onnx_runtime_content(ui, text);
        ui.add_space(4.0);
        render_vc_runtime_content(ui, text);
        ui.add_space(4.0);
        render_model_row(
            ui,
            text,
            &ModelRowSpec {
                model_probe: PROBE_PARAKEET_EOU,
                model_title: text.auxiliary.managed_tools.tool_parakeet,
                model_download_title: text.tool_runtime.parakeet_downloading_title.to_string(),
                model_notice: current_parakeet_model_notice,
                is_model_downloaded,
                model_dir: get_parakeet_model_dir,
                download_model: download_parakeet_model,
                remove_model: remove_parakeet_model,
                description: Some(text.auxiliary.managed_tools.tool_desc_parakeet),
                space_before_notice: true,
            },
        );
        ui.add_space(4.0);
        render_model_row(
            ui,
            text,
            &ModelRowSpec {
                model_probe: PROBE_PARAKEET_TDT,
                model_title: text.auxiliary.managed_tools.tool_parakeet_tdt,
                model_download_title: text.tool_runtime.parakeet_tdt_downloading_title.to_string(),
                model_notice: current_parakeet_tdt_model_notice,
                is_model_downloaded: is_parakeet_tdt_model_downloaded,
                model_dir: get_parakeet_tdt_model_dir,
                download_model: download_parakeet_tdt_model,
                remove_model: remove_parakeet_tdt_model,
                description: Some(text.auxiliary.managed_tools.tool_desc_parakeet_tdt),
                space_before_notice: true,
            },
        );
    });
}

pub(super) fn render_kokoro_card(ui: &mut egui::Ui, text: &LocaleText) {
    tool_card(ui, |ui| {
        ui.heading(text.auxiliary.managed_tools.tool_kokoro_card);
        ui.add_space(4.0);
        render_model_row(
            ui,
            text,
            &ModelRowSpec {
                model_probe: PROBE_KOKORO_V1,
                model_title: text.auxiliary.managed_tools.tool_kokoro,
                model_download_title: text.tool_runtime.kokoro_downloading_title.to_string(),
                model_notice: current_kokoro_model_notice,
                is_model_downloaded: is_kokoro_model_downloaded,
                model_dir: get_kokoro_model_dir,
                download_model: download_kokoro_model,
                remove_model: remove_kokoro_model,
                description: Some(text.auxiliary.managed_tools.tool_desc_kokoro),
                space_before_notice: true,
            },
        );
    });
}

pub(super) fn render_supertonic_card(ui: &mut egui::Ui, text: &LocaleText) {
    tool_card(ui, |ui| {
        ui.heading("Supertonic 3");
        ui.add_space(4.0);
        render_supertonic_content(ui, text);
    });
}

fn render_supertonic_content(ui: &mut egui::Ui, text: &LocaleText) {
    let theme = AppTheme::from_ui(ui);
    let notice = current_supertonic_model_notice();
    let download_title = crate::overlay::auto_copy_badge::format_locale(
        text.badge.downloading_model_fmt,
        &[("name", "Supertonic 3")],
    );
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(text.auxiliary.managed_tools.tool_supertonic_model).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let is_downloading = {
                if let Ok(state) = REALTIME_STATE.lock() {
                    state.is_downloading && state.download_title == download_title
                } else {
                    false
                }
            };

            if is_downloading {
                let progress = {
                    if let Ok(state) = REALTIME_STATE.lock() {
                        state.download_progress
                    } else {
                        0.0
                    }
                };
                ui.label(format!("{progress:.0}%"));
                ui.spinner();
            } else if cached_probe(PROBE_SUPERTONIC_3, is_supertonic_model_downloaded) {
                if ui
                    .button(
                        egui::RichText::new(text.auxiliary.managed_tools.tool_action_delete)
                            .color(theme.danger_text()),
                    )
                    .clicked()
                {
                    invalidate_size_cache(&get_supertonic_model_dir());
                    invalidate_probe_cache(PROBE_SUPERTONIC_3);
                    let _ = remove_supertonic_model();
                }
                let size = get_dir_size(&get_supertonic_model_dir());
                ui.label(
                    egui::RichText::new(
                        text.auxiliary
                            .managed_tools
                            .tool_status_installed
                            .replace("{}", &format_size(size)),
                    )
                    .color(theme.success()),
                );
            } else {
                if ui
                    .button(text.auxiliary.managed_tools.tool_action_download)
                    .clicked()
                {
                    let stop_signal = Arc::new(AtomicBool::new(false));
                    thread::spawn(move || {
                        let _ = download_supertonic_model(stop_signal, true);
                    });
                }
                ui.label(
                    egui::RichText::new(text.auxiliary.managed_tools.tool_status_missing)
                        .color(egui::Color32::GRAY),
                );
            }
        });
    });
    ui.label(
        text.auxiliary
            .managed_tools
            .tool_desc_supertonic_fmt
            .replace("{languages}", SUPERTONIC_LANGUAGE_SUMMARY),
    );
    if let Some(message) = notice {
        ui.add_space(4.0);
        ui.label(egui::RichText::new(message).color(theme.danger_text()));
    }
}

pub(super) fn render_qwen3_card(ui: &mut egui::Ui, text: &LocaleText) {
    tool_card(ui, |ui| {
        ui.heading(text.auxiliary.managed_tools.tool_qwen3_card);
        ui.add_space(4.0);
        render_qwen3_runtime_content(ui, text);
        ui.add_space(4.0);
        render_model_row(
            ui,
            text,
            &ModelRowSpec {
                model_probe: PROBE_QWEN3_SMALL,
                model_title: "Qwen3-ASR 0.6B",
                model_download_title: text.tool_runtime.qwen3_downloading_title.to_string(),
                model_notice: current_qwen3_model_notice,
                is_model_downloaded: is_qwen3_model_downloaded,
                model_dir: get_qwen3_model_dir,
                download_model: download_qwen3_model,
                remove_model: remove_qwen3_model,
                description: Some(text.auxiliary.managed_tools.tool_desc_qwen3),
                space_before_notice: true,
            },
        );
        ui.add_space(4.0);
        render_model_row(
            ui,
            text,
            &ModelRowSpec {
                model_probe: PROBE_QWEN3_LARGE,
                model_title: "Qwen3-ASR 1.7B",
                model_download_title: text.tool_runtime.qwen3_1_7b_downloading_title.to_string(),
                model_notice: no_model_notice,
                is_model_downloaded: is_qwen3_1_7b_model_downloaded,
                model_dir: get_qwen3_1_7b_model_dir,
                download_model: download_qwen3_1_7b_model,
                remove_model: remove_qwen3_1_7b_model,
                description: Some(text.auxiliary.managed_tools.tool_desc_qwen3_1_7b),
                space_before_notice: true,
            },
        );
    });
}

/// Notice hook for models that never surface a notice (Qwen3-ASR 1.7B).
fn no_model_notice() -> Option<String> {
    None
}

fn render_qwen3_runtime_content(ui: &mut egui::Ui, text: &LocaleText) {
    use crate::api::realtime_audio::qwen3::runtime::{
        active_qwen3_runtime_dir_for_display, current_qwen3_runtime_notice, download_qwen3_runtime,
        is_qwen3_runtime_downloading, is_qwen3_runtime_installed_for_display,
        qwen3_runtime_installed_size_for_display, remove_qwen3_runtime,
    };

    let theme = AppTheme::from_ui(ui);
    let runtime_notice = current_qwen3_runtime_notice();
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(text.auxiliary.managed_tools.tool_qwen3_runtime).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let is_downloading_runtime = is_qwen3_runtime_downloading() || {
                if let Ok(state) = REALTIME_STATE.lock() {
                    state.is_downloading
                        && state.download_title == text.tool_runtime.qwen3_runtime_downloading_title
                } else {
                    false
                }
            };

            if is_downloading_runtime {
                let progress = {
                    if let Ok(state) = REALTIME_STATE.lock() {
                        state.download_progress
                    } else {
                        0.0
                    }
                };
                ui.label(format!("{:.0}%", progress));
                ui.spinner();
            } else if cached_probe(PROBE_QWEN3_RUNTIME, is_qwen3_runtime_installed_for_display) {
                if ui
                    .button(
                        egui::RichText::new(text.auxiliary.managed_tools.tool_action_delete)
                            .color(theme.danger_text()),
                    )
                    .clicked()
                {
                    invalidate_probe_cache(PROBE_QWEN3_RUNTIME);
                    invalidate_probe_cache(PROBE_QWEN3_RUNTIME_ACTIVE);
                    invalidate_u64_cache(VALUE_QWEN3_RUNTIME_SIZE);
                    let _ = remove_qwen3_runtime();
                }
                let size = cached_u64(
                    VALUE_QWEN3_RUNTIME_SIZE,
                    qwen3_runtime_installed_size_for_display,
                );
                ui.label(
                    egui::RichText::new(
                        text.auxiliary
                            .managed_tools
                            .tool_status_installed
                            .replace("{}", &format_size(size)),
                    )
                    .color(theme.success()),
                );
            } else if cached_probe(PROBE_QWEN3_RUNTIME_ACTIVE, || {
                active_qwen3_runtime_dir_for_display().is_some()
            }) {
                if ui
                    .button(text.auxiliary.managed_tools.tool_action_download)
                    .clicked()
                {
                    let stop_signal = Arc::new(AtomicBool::new(false));
                    thread::spawn(move || {
                        let _ = download_qwen3_runtime(stop_signal, true);
                    });
                }
                let size = cached_u64(VALUE_QWEN3_RUNTIME_ACTIVE_SIZE, || {
                    active_qwen3_runtime_dir_for_display()
                        .map(|path| get_path_size(&path))
                        .unwrap_or(0)
                });
                ui.label(
                    egui::RichText::new(
                        text.auxiliary
                            .managed_tools
                            .tool_status_available_locally
                            .replace("{}", &format_size(size)),
                    )
                    .color(egui::Color32::from_rgb(96, 125, 139)),
                );
            } else {
                if ui
                    .button(text.auxiliary.managed_tools.tool_action_download)
                    .clicked()
                {
                    let stop_signal = Arc::new(AtomicBool::new(false));
                    thread::spawn(move || {
                        let _ = download_qwen3_runtime(stop_signal, true);
                    });
                }
                ui.label(
                    egui::RichText::new(text.auxiliary.managed_tools.tool_status_missing)
                        .color(egui::Color32::GRAY),
                );
            }
        });
    });
    ui.label(text.auxiliary.managed_tools.tool_desc_qwen3_runtime);
    if let Some(message) = runtime_notice {
        ui.add_space(4.0);
        ui.label(egui::RichText::new(message).color(theme.danger_text()));
    }
}
