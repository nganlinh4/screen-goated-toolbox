use crate::api::realtime_audio::sherpa_onnx::{self, ZipformerLanguage};
use eframe::egui;

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

pub(super) fn render_zipformer_section(ui: &mut egui::Ui) {
    ui.group(|ui| {
        ui.label(egui::RichText::new("Zipformer Models (sherpa-onnx)").strong());
        ui.add_space(4.0);

        // Runtime DLLs row
        let dlls_installed = sherpa_onnx::dlls::is_sherpa_dlls_installed();
        ui.horizontal(|ui| {
            ui.label("  Runtime DLLs (sherpa-onnx + onnxruntime)");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if dlls_installed {
                    if ui
                        .button(egui::RichText::new("Delete").color(egui::Color32::RED))
                        .clicked()
                    {
                        let _ = std::fs::remove_dir_all(sherpa_dlls_dir());
                    }
                    let size = get_dir_size(&sherpa_dlls_dir());
                    ui.label(
                        egui::RichText::new(format!("Installed ({})", format_size(size)))
                            .color(egui::Color32::from_rgb(34, 139, 34)),
                    );
                } else {
                    ui.label(
                        egui::RichText::new("Not installed — downloads automatically on first use")
                            .color(egui::Color32::GRAY),
                    );
                }
            });
        });
        ui.add_space(4.0);

        let mut any_installed = false;
        for &lang in ALL_LANGUAGES {
            let downloaded = sherpa_onnx::is_model_downloaded(lang);
            if downloaded {
                any_installed = true;
            }

            ui.horizontal(|ui| {
                let label = format!("  {} ({})", lang.display_name(), lang.code());
                ui.label(label);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if downloaded {
                        if ui
                            .button(egui::RichText::new("Delete").color(egui::Color32::RED))
                            .clicked()
                        {
                            let dir = model_dir(lang);
                            let _ = std::fs::remove_dir_all(&dir);
                        }
                        let size = get_dir_size(&model_dir(lang));
                        ui.label(
                            egui::RichText::new(format!("Installed ({})", format_size(size)))
                                .color(egui::Color32::from_rgb(34, 139, 34)),
                        );
                    } else {
                        ui.label(egui::RichText::new("Not downloaded").color(egui::Color32::GRAY));
                    }
                });
            });
        }

        if !any_installed {
            ui.add_space(2.0);
            ui.label(
                egui::RichText::new(
                    "Models download automatically when selected in the transcription overlay.",
                )
                .color(egui::Color32::GRAY)
                .italics(),
            );
        }
    });
}
