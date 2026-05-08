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
use crate::api::realtime_audio::qwen3::server::{
    current_qwen3_server_notice, download_qwen3_server, get_active_qwen3_server_path,
    get_qwen3_server_path, is_qwen3_server_managed, remove_qwen3_server,
};
use crate::gui::locale::LocaleText;
use crate::overlay::realtime_webview::state::REALTIME_STATE;
use eframe::egui;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::thread;

use super::ai_runtime::render_ai_runtime_content;
use super::utils::{
    cached_probe, cached_u64, format_size, get_dir_size, get_path_size, invalidate_probe_cache,
    invalidate_size_cache, invalidate_u64_cache,
};

const PROBE_PARAKEET_EOU: &str = "downloaded-tools:parakeet-eou";
const PROBE_PARAKEET_TDT: &str = "downloaded-tools:parakeet-tdt";
const PROBE_QWEN3_SMALL: &str = "downloaded-tools:qwen3-small";
const PROBE_QWEN3_LARGE: &str = "downloaded-tools:qwen3-large";
const PROBE_QWEN3_RUNTIME: &str = "downloaded-tools:qwen3-runtime";
const PROBE_QWEN3_SERVER_MANAGED: &str = "downloaded-tools:qwen3-server-managed";
const PROBE_QWEN3_SERVER_ACTIVE: &str = "downloaded-tools:qwen3-server-active";
const PROBE_QWEN3_RUNTIME_ACTIVE: &str = "downloaded-tools:qwen3-runtime-active";
const VALUE_QWEN3_RUNTIME_SIZE: &str = "downloaded-tools:qwen3-runtime-size";
const VALUE_QWEN3_SERVER_ACTIVE_SIZE: &str = "downloaded-tools:qwen3-server-active-size";
const VALUE_QWEN3_RUNTIME_ACTIVE_SIZE: &str = "downloaded-tools:qwen3-runtime-active-size";

pub(super) fn render_parakeet_card(ui: &mut egui::Ui, text: &LocaleText) {
    ui.group(|ui| {
        ui.heading(text.tool_parakeet_card);
        ui.add_space(4.0);
        render_ai_runtime_content(ui, text);
        ui.add_space(4.0);
        render_parakeet_content(ui, text);
        ui.add_space(4.0);
        render_parakeet_tdt_content(ui, text);
    });
}

pub(super) fn render_qwen3_card(ui: &mut egui::Ui, text: &LocaleText) {
    ui.group(|ui| {
        ui.heading(text.tool_qwen3_card);
        ui.add_space(4.0);
        render_qwen3_runtime_content(ui, text);
        ui.add_space(4.0);
        render_qwen3_server_content(ui, text);
        ui.add_space(4.0);
        render_qwen3_content(ui, text);
        ui.add_space(4.0);
        render_qwen3_1_7b_content(ui, text);
    });
}

fn render_parakeet_content(ui: &mut egui::Ui, text: &LocaleText) {
    let parakeet_notice = current_parakeet_model_notice();
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(text.tool_parakeet).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let is_downloading = {
                if let Ok(state) = REALTIME_STATE.lock() {
                    state.is_downloading && state.download_title == text.parakeet_downloading_title
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
                ui.label(format!("{:.0}%", progress));
                ui.spinner();
            } else if cached_probe(PROBE_PARAKEET_EOU, is_model_downloaded) {
                if ui
                    .button(egui::RichText::new(text.tool_action_delete).color(egui::Color32::RED))
                    .clicked()
                {
                    invalidate_size_cache(&get_parakeet_model_dir());
                    invalidate_probe_cache(PROBE_PARAKEET_EOU);
                    let _ = remove_parakeet_model();
                }
                let size = get_dir_size(&get_parakeet_model_dir());
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
                        let _ = download_parakeet_model(stop_signal, false);
                    });
                }
                ui.label(egui::RichText::new(text.tool_status_missing).color(egui::Color32::GRAY));
            }
        });
    });
    ui.label(text.tool_desc_parakeet);
    if let Some(message) = parakeet_notice {
        ui.add_space(4.0);
        ui.label(egui::RichText::new(message).color(egui::Color32::RED));
    }
}

fn render_parakeet_tdt_content(ui: &mut egui::Ui, text: &LocaleText) {
    let parakeet_notice = current_parakeet_tdt_model_notice();
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(text.tool_parakeet_tdt).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let is_downloading = {
                if let Ok(state) = REALTIME_STATE.lock() {
                    state.is_downloading
                        && state.download_title == text.parakeet_tdt_downloading_title
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
            } else if cached_probe(PROBE_PARAKEET_TDT, is_parakeet_tdt_model_downloaded) {
                if ui
                    .button(egui::RichText::new(text.tool_action_delete).color(egui::Color32::RED))
                    .clicked()
                {
                    invalidate_size_cache(&get_parakeet_tdt_model_dir());
                    invalidate_probe_cache(PROBE_PARAKEET_TDT);
                    let _ = remove_parakeet_tdt_model();
                }
                let size = get_dir_size(&get_parakeet_tdt_model_dir());
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
                        let _ = download_parakeet_tdt_model(stop_signal, false);
                    });
                }
                ui.label(egui::RichText::new(text.tool_status_missing).color(egui::Color32::GRAY));
            }
        });
    });
    ui.label(text.tool_desc_parakeet_tdt);
    if let Some(message) = parakeet_notice {
        ui.add_space(4.0);
        ui.label(egui::RichText::new(message).color(egui::Color32::RED));
    }
}

fn render_qwen3_content(ui: &mut egui::Ui, text: &LocaleText) {
    let qwen_notice = current_qwen3_model_notice();
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Qwen3-ASR 0.6B").strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let is_downloading = {
                if let Ok(state) = REALTIME_STATE.lock() {
                    state.is_downloading && state.download_title == text.qwen3_downloading_title
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
            } else if cached_probe(PROBE_QWEN3_SMALL, is_qwen3_model_downloaded) {
                if ui
                    .button(egui::RichText::new(text.tool_action_delete).color(egui::Color32::RED))
                    .clicked()
                {
                    invalidate_size_cache(&get_qwen3_model_dir());
                    invalidate_probe_cache(PROBE_QWEN3_SMALL);
                    let _ = remove_qwen3_model();
                }
                let size = get_dir_size(&get_qwen3_model_dir());
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
                        let _ = download_qwen3_model(stop_signal, false);
                    });
                }
                ui.label(egui::RichText::new(text.tool_status_missing).color(egui::Color32::GRAY));
            }
        });
    });
    ui.label(text.tool_desc_qwen3);
    if let Some(message) = qwen_notice {
        ui.add_space(4.0);
        ui.label(egui::RichText::new(message).color(egui::Color32::RED));
    }
}

fn render_qwen3_1_7b_content(ui: &mut egui::Ui, text: &LocaleText) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Qwen3-ASR 1.7B").strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let is_downloading = {
                if let Ok(state) = REALTIME_STATE.lock() {
                    state.is_downloading
                        && state.download_title == text.qwen3_1_7b_downloading_title
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
            } else if cached_probe(PROBE_QWEN3_LARGE, is_qwen3_1_7b_model_downloaded) {
                if ui
                    .button(egui::RichText::new(text.tool_action_delete).color(egui::Color32::RED))
                    .clicked()
                {
                    invalidate_size_cache(&get_qwen3_1_7b_model_dir());
                    invalidate_probe_cache(PROBE_QWEN3_LARGE);
                    let _ = remove_qwen3_1_7b_model();
                }
                let size = get_dir_size(&get_qwen3_1_7b_model_dir());
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
                        let _ = download_qwen3_1_7b_model(stop_signal, false);
                    });
                }
                ui.label(egui::RichText::new(text.tool_status_missing).color(egui::Color32::GRAY));
            }
        });
    });
    ui.label(text.tool_desc_qwen3_1_7b);
}

fn render_qwen3_server_content(ui: &mut egui::Ui, text: &LocaleText) {
    let server_notice = current_qwen3_server_notice();
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(text.tool_qwen3_server).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let is_downloading_server = {
                if let Ok(state) = REALTIME_STATE.lock() {
                    state.is_downloading
                        && state.download_title == text.qwen3_server_downloading_title
                } else {
                    false
                }
            };

            if is_downloading_server {
                let progress = {
                    if let Ok(state) = REALTIME_STATE.lock() {
                        state.download_progress
                    } else {
                        0.0
                    }
                };
                ui.label(format!("{progress:.0}%"));
                ui.spinner();
            } else if cached_probe(PROBE_QWEN3_SERVER_MANAGED, is_qwen3_server_managed) {
                if ui
                    .button(egui::RichText::new(text.tool_action_delete).color(egui::Color32::RED))
                    .clicked()
                {
                    invalidate_size_cache(&get_qwen3_server_path());
                    invalidate_probe_cache(PROBE_QWEN3_SERVER_MANAGED);
                    invalidate_probe_cache(PROBE_QWEN3_SERVER_ACTIVE);
                    let _ = remove_qwen3_server();
                }
                let size = get_path_size(&get_qwen3_server_path());
                ui.label(
                    egui::RichText::new(
                        text.tool_status_installed.replace("{}", &format_size(size)),
                    )
                    .color(egui::Color32::from_rgb(34, 139, 34)),
                );
            } else if cached_probe(PROBE_QWEN3_SERVER_ACTIVE, || {
                get_active_qwen3_server_path().is_some()
            }) {
                if ui.button(text.tool_action_download).clicked() {
                    let stop_signal = Arc::new(AtomicBool::new(false));
                    thread::spawn(move || {
                        let _ = download_qwen3_server(stop_signal, false);
                    });
                }
                let size = cached_u64(VALUE_QWEN3_SERVER_ACTIVE_SIZE, || {
                    get_active_qwen3_server_path()
                        .map(|path| get_path_size(&path))
                        .unwrap_or(0)
                });
                ui.label(
                    egui::RichText::new(
                        text.tool_status_available_locally
                            .replace("{}", &format_size(size)),
                    )
                    .color(egui::Color32::from_rgb(96, 125, 139)),
                );
            } else {
                if ui.button(text.tool_action_download).clicked() {
                    let stop_signal = Arc::new(AtomicBool::new(false));
                    thread::spawn(move || {
                        let _ = download_qwen3_server(stop_signal, false);
                    });
                }
                ui.label(egui::RichText::new(text.tool_status_missing).color(egui::Color32::GRAY));
            }
        });
    });
    ui.label(text.tool_desc_qwen3_server);
    if let Some(message) = server_notice {
        ui.add_space(4.0);
        ui.label(egui::RichText::new(message).color(egui::Color32::RED));
    }
}

fn render_qwen3_runtime_content(ui: &mut egui::Ui, text: &LocaleText) {
    use crate::api::realtime_audio::qwen3::runtime::{
        active_qwen3_runtime_dir, current_qwen3_runtime_notice, download_qwen3_runtime,
        is_qwen3_runtime_downloading, is_qwen3_runtime_managed_installed,
        qwen3_runtime_installed_size, remove_qwen3_runtime,
    };

    let runtime_notice = current_qwen3_runtime_notice();
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(text.tool_qwen3_runtime).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let is_downloading_runtime = is_qwen3_runtime_downloading() || {
                if let Ok(state) = REALTIME_STATE.lock() {
                    state.is_downloading
                        && state.download_title == text.qwen3_runtime_downloading_title
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
            } else if cached_probe(PROBE_QWEN3_RUNTIME, is_qwen3_runtime_managed_installed) {
                if ui
                    .button(egui::RichText::new(text.tool_action_delete).color(egui::Color32::RED))
                    .clicked()
                {
                    invalidate_probe_cache(PROBE_QWEN3_RUNTIME);
                    invalidate_probe_cache(PROBE_QWEN3_RUNTIME_ACTIVE);
                    invalidate_u64_cache(VALUE_QWEN3_RUNTIME_SIZE);
                    let _ = remove_qwen3_runtime();
                }
                let size = cached_u64(VALUE_QWEN3_RUNTIME_SIZE, qwen3_runtime_installed_size);
                ui.label(
                    egui::RichText::new(
                        text.tool_status_installed.replace("{}", &format_size(size)),
                    )
                    .color(egui::Color32::from_rgb(34, 139, 34)),
                );
            } else if cached_probe(PROBE_QWEN3_RUNTIME_ACTIVE, || {
                active_qwen3_runtime_dir().is_some()
            }) {
                if ui.button(text.tool_action_download).clicked() {
                    let stop_signal = Arc::new(AtomicBool::new(false));
                    thread::spawn(move || {
                        let _ = download_qwen3_runtime(stop_signal, false);
                    });
                }
                let size = cached_u64(VALUE_QWEN3_RUNTIME_ACTIVE_SIZE, || {
                    active_qwen3_runtime_dir()
                        .map(|path| get_path_size(&path))
                        .unwrap_or(0)
                });
                ui.label(
                    egui::RichText::new(
                        text.tool_status_available_locally
                            .replace("{}", &format_size(size)),
                    )
                    .color(egui::Color32::from_rgb(96, 125, 139)),
                );
            } else {
                if ui.button(text.tool_action_download).clicked() {
                    let stop_signal = Arc::new(AtomicBool::new(false));
                    thread::spawn(move || {
                        let _ = download_qwen3_runtime(stop_signal, false);
                    });
                }
                ui.label(egui::RichText::new(text.tool_status_missing).color(egui::Color32::GRAY));
            }
        });
    });
    ui.label(text.tool_desc_qwen3_runtime);
    if let Some(message) = runtime_notice {
        ui.add_space(4.0);
        ui.label(egui::RichText::new(message).color(egui::Color32::RED));
    }
}
