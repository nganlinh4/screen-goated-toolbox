use crate::api::realtime_audio::sherpa_onnx::{self, ZipformerLanguage};
use crate::gui::locale::LocaleText;
use crate::gui::settings_ui::download_manager::{DownloadManager, InstallStatus};
use eframe::egui;

use super::utils::{format_size, get_dir_size};

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

pub(super) fn render_zipformer_section(
    ui: &mut egui::Ui,
    download_manager: &mut DownloadManager,
    text: &LocaleText,
) {
    ui.group(|ui| {
        ui.heading(text.tool_zipformer_card);
        ui.add_space(4.0);

        // Sync disk state → status (when not actively downloading)
        {
            let disk_ok = sherpa_onnx::dlls::is_sherpa_dlls_installed();
            let mut s = download_manager.zipformer_dlls_status.lock().unwrap();
            let in_dl = matches!(*s, InstallStatus::Downloading(_));
            if !in_dl {
                match (&*s, disk_ok) {
                    (InstallStatus::Installed, false) => *s = InstallStatus::Missing,
                    (InstallStatus::Missing, true)
                    | (InstallStatus::Checking, true)
                    | (InstallStatus::Error(_), true) => *s = InstallStatus::Installed,
                    (InstallStatus::Checking, false) => *s = InstallStatus::Missing,
                    _ => {}
                }
            }
        }
        let dlls_status = download_manager.zipformer_dlls_status.lock().unwrap().clone();

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(text.tool_zipformer_runtime_dlls).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                match dlls_status {
                    InstallStatus::Installed => {
                        if ui
                            .button(
                                egui::RichText::new(text.tool_action_delete)
                                    .color(egui::Color32::RED),
                            )
                            .clicked()
                        {
                            let _ = std::fs::remove_dir_all(sherpa_onnx::dlls::sherpa_bin_dir());
                            *download_manager.zipformer_dlls_status.lock().unwrap() =
                                InstallStatus::Missing;
                        }
                        let size = get_dir_size(&sherpa_onnx::dlls::sherpa_bin_dir());
                        ui.label(
                            egui::RichText::new(
                                text.tool_status_installed
                                    .replace("{}", &format_size(size)),
                            )
                            .color(egui::Color32::from_rgb(34, 139, 34)),
                        );
                    }
                    InstallStatus::Downloading(p) => {
                        ui.spinner();
                        ui.label(format!("{:.0}%", p * 100.0));
                    }
                    InstallStatus::Extracting => {
                        ui.spinner();
                        ui.label(text.download_status_extracting);
                    }
                    InstallStatus::Checking => {
                        ui.spinner();
                    }
                    _ => {
                        if ui.button(text.tool_action_download).clicked() {
                            download_manager.start_download_sherpa_dlls();
                        }
                        ui.label(
                            egui::RichText::new(text.tool_status_missing)
                                .color(egui::Color32::GRAY),
                        );
                    }
                }
            });
        });
        ui.label(
            egui::RichText::new(text.tool_zipformer_desc_runtime_dlls)
                .color(egui::Color32::GRAY)
                .small(),
        );
        ui.add_space(4.0);

        for &lang in ALL_LANGUAGES {
            // Sync disk → status
            {
                let disk_ok = sherpa_onnx::is_model_downloaded(lang);
                let mut s = download_manager.zipformer_lang_statuses[&lang]
                    .lock()
                    .unwrap();
                let in_dl = matches!(*s, InstallStatus::Downloading(_));
                if !in_dl {
                    match (&*s, disk_ok) {
                        (InstallStatus::Installed, false) => *s = InstallStatus::Missing,
                        (InstallStatus::Missing, true)
                        | (InstallStatus::Checking, true)
                        | (InstallStatus::Error(_), true) => *s = InstallStatus::Installed,
                        (InstallStatus::Checking, false) => *s = InstallStatus::Missing,
                        _ => {}
                    }
                }
            }
            let status = download_manager.zipformer_lang_statuses[&lang]
                .lock()
                .unwrap()
                .clone();
            let label = format!("{} ({})", lang.display_name(), lang.code());

            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(&label).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    match status {
                        InstallStatus::Installed => {
                            if ui
                                .button(
                                    egui::RichText::new(text.tool_action_delete)
                                        .color(egui::Color32::RED),
                                )
                                .clicked()
                            {
                                let _ = std::fs::remove_dir_all(model_dir(lang));
                                *download_manager.zipformer_lang_statuses[&lang]
                                    .lock()
                                    .unwrap() = InstallStatus::Missing;
                            }
                            let size = get_dir_size(&model_dir(lang));
                            ui.label(
                                egui::RichText::new(
                                    text.tool_status_installed
                                        .replace("{}", &format_size(size)),
                                )
                                .color(egui::Color32::from_rgb(34, 139, 34)),
                            );
                        }
                        InstallStatus::Downloading(p) => {
                            ui.spinner();
                            ui.label(format!("{:.0}%", p * 100.0));
                        }
                        InstallStatus::Checking => {
                            ui.spinner();
                        }
                        _ => {
                            if ui.button(text.tool_action_download).clicked() {
                                download_manager.start_download_zipformer_lang(lang);
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
