use crate::api::realtime_audio::model_loader::{
    current_parakeet_model_notice, download_parakeet_model, get_parakeet_model_dir,
    is_model_downloaded, remove_parakeet_model,
};
use crate::api::realtime_audio::qwen3::assets::{
    current_qwen3_model_notice, download_qwen3_1_7b_model, download_qwen3_model,
    get_qwen3_1_7b_model_dir, get_qwen3_model_dir, is_qwen3_1_7b_model_downloaded,
    is_qwen3_model_downloaded, remove_qwen3_1_7b_model, remove_qwen3_model,
};
use crate::api::realtime_audio::qwen3::server::{
    current_qwen3_server_notice, download_qwen3_server, get_active_qwen3_server_path,
    is_qwen3_server_downloaded, is_qwen3_server_managed, remove_qwen3_server,
};
use crate::gui::locale::LocaleText;
use crate::overlay::realtime_webview::state::REALTIME_STATE;
use eframe::egui;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::thread;

use super::ai_runtime::render_ai_runtime_content;
use super::utils::{format_size, get_dir_size, get_path_size};

pub(super) fn render_parakeet_card(ui: &mut egui::Ui, text: &LocaleText) {
    ui.group(|ui| {
        ui.heading(text.tool_parakeet_card);
        ui.add_space(4.0);
        render_ai_runtime_content(ui, text);
        ui.add_space(4.0);
        render_parakeet_content(ui, text);
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
            } else if is_model_downloaded() {
                if ui
                    .button(egui::RichText::new(text.tool_action_delete).color(egui::Color32::RED))
                    .clicked()
                {
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

fn render_qwen3_content(ui: &mut egui::Ui, text: &LocaleText) {
    let qwen_notice = current_qwen3_model_notice();
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Qwen3-ASR 0.6B").strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if is_qwen3_model_downloaded() {
                if ui
                    .button(egui::RichText::new(text.tool_action_delete).color(egui::Color32::RED))
                    .clicked()
                {
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
            if is_qwen3_1_7b_model_downloaded() {
                if ui
                    .button(egui::RichText::new(text.tool_action_delete).color(egui::Color32::RED))
                    .clicked()
                {
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
            } else if is_qwen3_server_downloaded() {
                if is_qwen3_server_managed()
                    && ui
                        .button(
                            egui::RichText::new(text.tool_action_delete).color(egui::Color32::RED),
                        )
                        .clicked()
                {
                    let _ = remove_qwen3_server();
                }
                let size = get_active_qwen3_server_path()
                    .map(|path| get_path_size(&path))
                    .unwrap_or(0);
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
        current_qwen3_runtime_notice, download_qwen3_runtime, is_qwen3_runtime_downloading,
        is_qwen3_runtime_managed_installed, qwen3_runtime_installed_size, remove_qwen3_runtime,
    };

    let runtime_notice = current_qwen3_runtime_notice();
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(text.tool_qwen3_runtime).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let is_downloading_runtime = is_qwen3_runtime_downloading() || {
                if let Ok(state) = REALTIME_STATE.lock() {
                    state.is_downloading && state.download_title.contains("CUDA Runtime")
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
            } else if is_qwen3_runtime_managed_installed() {
                if ui
                    .button(egui::RichText::new(text.tool_action_delete).color(egui::Color32::RED))
                    .clicked()
                {
                    let _ = remove_qwen3_runtime();
                }
                let size = qwen3_runtime_installed_size();
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
