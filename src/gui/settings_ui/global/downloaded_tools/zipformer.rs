use crate::api::realtime_audio::sherpa_onnx::{self, ZipformerLanguage};
use crate::gui::locale::LocaleText;
use eframe::egui;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use super::utils::{format_size, get_dir_size};

fn sherpa_dlls_dir() -> std::path::PathBuf {
    crate::unpack_dlls::private_bin_dir().join("sherpa-onnx")
}

const ALL_LANGUAGES: &[ZipformerLanguage] = &[
    ZipformerLanguage::English,
    ZipformerLanguage::Korean,
    ZipformerLanguage::Chinese,
    ZipformerLanguage::French,
    ZipformerLanguage::German,
    ZipformerLanguage::Spanish,
    ZipformerLanguage::Russian,
    ZipformerLanguage::All8Lang,
];

fn model_dir(lang: ZipformerLanguage) -> std::path::PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("screen-goated-toolbox")
        .join("models")
        .join(lang.model_dir_name())
}

pub(super) fn render_zipformer_section(ui: &mut egui::Ui, text: &LocaleText) {
    ui.group(|ui| {
        ui.label(egui::RichText::new("Zipformer ASR (sherpa-onnx)").strong());
        ui.add_space(4.0);

        // Runtime DLLs row
        let dlls_installed = sherpa_onnx::dlls::is_sherpa_dlls_installed();
        ui.horizontal(|ui| {
            ui.label("  Runtime DLLs (sherpa-onnx + onnxruntime)");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if dlls_installed {
                    if ui
                        .button(
                            egui::RichText::new(text.tool_action_delete)
                                .color(egui::Color32::RED),
                        )
                        .clicked()
                    {
                        let _ = std::fs::remove_dir_all(sherpa_dlls_dir());
                    }
                    let size = get_dir_size(&sherpa_dlls_dir());
                    ui.label(
                        egui::RichText::new(
                            text.tool_status_installed
                                .replace("{}", &format_size(size)),
                        )
                        .color(egui::Color32::from_rgb(34, 139, 34)),
                    );
                } else {
                    ui.label(
                        egui::RichText::new(text.tool_status_missing)
                            .color(egui::Color32::GRAY),
                    );
                }
            });
        });
        ui.add_space(4.0);

        for &lang in ALL_LANGUAGES {
            let downloaded = sherpa_onnx::is_model_downloaded(lang);
            let label = format!("  {} ({})", lang.display_name(), lang.code());

            ui.horizontal(|ui| {
                ui.label(&label);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if downloaded {
                        if ui
                            .button(
                                egui::RichText::new(text.tool_action_delete)
                                    .color(egui::Color32::RED),
                            )
                            .clicked()
                        {
                            let _ = std::fs::remove_dir_all(model_dir(lang));
                        }
                        let size = get_dir_size(&model_dir(lang));
                        ui.label(
                            egui::RichText::new(
                                text.tool_status_installed
                                    .replace("{}", &format_size(size)),
                            )
                            .color(egui::Color32::from_rgb(34, 139, 34)),
                        );
                    } else {
                        let stop = Arc::new(AtomicBool::new(false));
                        let is_downloading = {
                            use crate::overlay::realtime_webview::state::REALTIME_STATE;
                            if let Ok(state) = REALTIME_STATE.lock() {
                                state.is_downloading
                                    && state
                                        .download_title
                                        .contains(lang.display_name())
                            } else {
                                false
                            }
                        };

                        if is_downloading {
                            let progress = {
                                use crate::overlay::realtime_webview::state::REALTIME_STATE;
                                if let Ok(state) = REALTIME_STATE.lock() {
                                    state.download_progress
                                } else {
                                    0.0
                                }
                            };
                            ui.label(format!("{:.0}%", progress));
                            ui.spinner();
                        } else {
                            if ui.button(text.tool_action_download).clicked() {
                                let stop_signal = stop.clone();
                                std::thread::spawn(move || {
                                    let _ = sherpa_onnx::download_model(
                                        lang,
                                        &stop_signal,
                                        windows::Win32::Foundation::HWND::default(),
                                    );
                                });
                            }
                            ui.label(
                                egui::RichText::new(text.tool_status_missing)
                                    .color(egui::Color32::GRAY),
                            );
                        }
                    }
                });
            });
        }
    });
}
