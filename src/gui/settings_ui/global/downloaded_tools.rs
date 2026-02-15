use crate::api::realtime_audio::model_loader::{
    download_parakeet_model, get_parakeet_model_dir, is_model_downloaded,
};
use crate::gui::locale::LocaleText;
use crate::gui::settings_ui::download_manager::{DownloadManager, InstallStatus, UpdateStatus};
use crate::overlay::realtime_webview::state::REALTIME_STATE;
use eframe::egui;
use std::fs;
use std::path::PathBuf;

use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::thread;

pub fn render_downloaded_tools_modal(
    ctx: &egui::Context,
    _ui: &mut egui::Ui,
    show_modal: &mut bool,
    download_manager: &mut DownloadManager,
    text: &LocaleText,
) {
    if *show_modal {
        let mut open = true;
        egui::Window::new(text.downloaded_tools_title)
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(500.0)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.add_space(8.0);

                // --- Parakeet ---
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(text.tool_parakeet).strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let is_downloading = {
                                if let Ok(state) = REALTIME_STATE.lock() {
                                    state.is_downloading
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
                                    .button(
                                        egui::RichText::new(text.tool_action_delete)
                                            .color(egui::Color32::RED),
                                    )
                                    .clicked()
                                {
                                    let _ = fs::remove_dir_all(get_parakeet_model_dir());
                                }
                                let size = get_dir_size(get_parakeet_model_dir());
                                ui.label(
                                    egui::RichText::new(
                                        text.tool_status_installed
                                            .replace("{}", &format_size(size)),
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
                                ui.label(
                                    egui::RichText::new(text.tool_status_missing)
                                        .color(egui::Color32::GRAY),
                                );
                            }
                        });
                    });
                    ui.label(text.tool_desc_parakeet);
                });

                ui.add_space(8.0);

                // --- yt-dlp ---
                ui.group(|ui| {
                    let status = download_manager.ytdlp_status.lock().unwrap().clone();
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(text.tool_ytdlp).strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            match status {
                                InstallStatus::Installed => {
                                    let path = download_manager.bin_dir.join("yt-dlp.exe");
                                    if ui
                                        .button(
                                            egui::RichText::new(text.tool_action_delete)
                                                .color(egui::Color32::RED),
                                        )
                                        .clicked()
                                    {
                                        let _ = fs::remove_file(path);
                                        *download_manager.ytdlp_status.lock().unwrap() =
                                            InstallStatus::Missing;
                                    }

                                    let size = if let Ok(meta) =
                                        fs::metadata(download_manager.bin_dir.join("yt-dlp.exe"))
                                    {
                                        meta.len()
                                    } else {
                                        0
                                    };
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
                                        download_manager.start_download_ytdlp();
                                    }
                                    ui.label(
                                        egui::RichText::new(text.tool_status_missing)
                                            .color(egui::Color32::GRAY),
                                    );
                                }
                            }
                        });
                    });

                    ui.horizontal(|ui| {
                        ui.label(text.tool_desc_ytdlp);
                        if matches!(status, InstallStatus::Installed) {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    // Update Status
                                    let u_status = {
                                        if let Ok(s) = download_manager.ytdlp_update_status.lock() {
                                            s.clone()
                                        } else {
                                            UpdateStatus::Idle
                                        }
                                    };

                                    match u_status {
                                        UpdateStatus::UpdateAvailable(ver) => {
                                            if ui
                                                .button(
                                                    egui::RichText::new(
                                                        text.tool_update_available
                                                            .replace("{}", &ver),
                                                    )
                                                    .color(egui::Color32::from_rgb(255, 165, 0)),
                                                )
                                                .clicked()
                                            {
                                                download_manager.start_download_ytdlp();
                                            }
                                        }
                                        UpdateStatus::Checking => {
                                            ui.spinner();
                                            ui.label(text.tool_update_checking);
                                        }
                                        UpdateStatus::UpToDate => {
                                            if ui
                                                .small_button(text.tool_update_check_again)
                                                .clicked()
                                            {
                                                download_manager.check_updates();
                                            }
                                            ui.label(
                                                egui::RichText::new(text.tool_update_latest)
                                                    .color(egui::Color32::from_rgb(34, 139, 34)),
                                            );
                                        }
                                        UpdateStatus::Error(e) => {
                                            if ui.small_button(text.tool_update_retry).clicked() {
                                                download_manager.check_updates();
                                            }
                                            ui.label(
                                                egui::RichText::new(text.tool_update_error)
                                                    .color(egui::Color32::RED),
                                            )
                                            .on_hover_text(e);
                                        }
                                        UpdateStatus::Idle => {
                                            if ui.small_button(text.tool_update_check_btn).clicked()
                                            {
                                                download_manager.check_updates();
                                            }
                                        }
                                    }

                                    // Version
                                    if let Ok(guard) = download_manager.ytdlp_version.lock() {
                                        if let Some(ver) = &*guard {
                                            ui.label(
                                                egui::RichText::new(format!("v{}", ver))
                                                    .color(egui::Color32::GRAY),
                                            );
                                        }
                                    }
                                },
                            );
                        }
                    });
                });

                ui.add_space(8.0);

                // --- ffmpeg ---
                ui.group(|ui| {
                    let ffmpeg_exists = download_manager.bin_dir.join("ffmpeg.exe").exists();
                    let ffprobe_exists = download_manager.bin_dir.join("ffprobe.exe").exists();
                    let installed_on_disk = ffmpeg_exists && ffprobe_exists;

                    // Keep UI status in sync when ffmpeg is installed externally
                    // (e.g., from the screen recorder panel).
                    {
                        let mut s = download_manager.ffmpeg_status.lock().unwrap();
                        let in_downloading_state =
                            matches!(*s, InstallStatus::Downloading(_) | InstallStatus::Extracting);
                        if !in_downloading_state {
                            match (&*s, installed_on_disk) {
                                (InstallStatus::Installed, false) => *s = InstallStatus::Missing,
                                (InstallStatus::Missing, true) => *s = InstallStatus::Installed,
                                (InstallStatus::Checking, true) => *s = InstallStatus::Installed,
                                (InstallStatus::Checking, false) => *s = InstallStatus::Missing,
                                (InstallStatus::Error(_), true) => *s = InstallStatus::Installed,
                                _ => {}
                            }
                        }
                    }

                    let status = download_manager.ffmpeg_status.lock().unwrap().clone();
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(text.tool_ffmpeg).strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            match status {
                                InstallStatus::Installed => {
                                    let path = download_manager.bin_dir.join("ffmpeg.exe");
                                    if ui
                                        .button(
                                            egui::RichText::new(text.tool_action_delete)
                                                .color(egui::Color32::RED),
                                        )
                                        .clicked()
                                    {
                                        let _ = fs::remove_file(&path);
                                        let _ = fs::remove_file(
                                            download_manager.bin_dir.join("ffprobe.exe"),
                                        );
                                        let _ = fs::remove_file(
                                            download_manager
                                                .bin_dir
                                                .join("ffmpeg_release_source.txt"),
                                        );
                                        *download_manager.ffmpeg_status.lock().unwrap() =
                                            InstallStatus::Missing;
                                    }

                                    let size = {
                                        let mut total = 0;
                                        if let Ok(meta) = fs::metadata(
                                            download_manager.bin_dir.join("ffmpeg.exe"),
                                        ) {
                                            total += meta.len();
                                        }
                                        if let Ok(meta) = fs::metadata(
                                            download_manager.bin_dir.join("ffprobe.exe"),
                                        ) {
                                            total += meta.len();
                                        }
                                        total
                                    };
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
                                        download_manager.start_download_ffmpeg();
                                    }
                                    ui.label(
                                        egui::RichText::new(text.tool_status_missing)
                                            .color(egui::Color32::GRAY),
                                    );
                                }
                            }
                        });
                    });

                    ui.horizontal(|ui| {
                        ui.label(text.tool_desc_ffmpeg);
                        if matches!(status, InstallStatus::Installed) {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    // Update Status
                                    let u_status = {
                                        if let Ok(s) = download_manager.ffmpeg_update_status.lock()
                                        {
                                            s.clone()
                                        } else {
                                            UpdateStatus::Idle
                                        }
                                    };

                                    match u_status {
                                        UpdateStatus::UpdateAvailable(ver) => {
                                            if ui
                                                .button(
                                                    egui::RichText::new(
                                                        text.tool_update_available
                                                            .replace("{}", &ver),
                                                    )
                                                    .color(egui::Color32::from_rgb(255, 165, 0)),
                                                )
                                                .clicked()
                                            {
                                                download_manager.start_download_ffmpeg();
                                            }
                                        }
                                        UpdateStatus::Checking => {
                                            ui.spinner();
                                            ui.label(text.tool_update_checking);
                                        }
                                        UpdateStatus::UpToDate => {
                                            if ui
                                                .small_button(text.tool_update_check_again)
                                                .clicked()
                                            {
                                                download_manager.check_updates();
                                            }
                                            ui.label(
                                                egui::RichText::new(text.tool_update_latest)
                                                    .color(egui::Color32::from_rgb(34, 139, 34)),
                                            );
                                        }
                                        UpdateStatus::Error(e) => {
                                            if ui.small_button(text.tool_update_retry).clicked() {
                                                download_manager.check_updates();
                                            }
                                            ui.label(
                                                egui::RichText::new(text.tool_update_error)
                                                    .color(egui::Color32::RED),
                                            )
                                            .on_hover_text(e);
                                        }
                                        UpdateStatus::Idle => {
                                            if ui.small_button(text.tool_update_check_btn).clicked()
                                            {
                                                download_manager.check_updates();
                                            }
                                        }
                                    }

                                    // Version
                                    if let Ok(guard) = download_manager.ffmpeg_version.lock() {
                                        if let Some(ver) = &*guard {
                                            ui.label(
                                                egui::RichText::new(format!("v{}", ver))
                                                    .color(egui::Color32::GRAY),
                                            );
                                        }
                                    }
                                },
                            );
                        }
                    });
                });
            });

        *show_modal = open;
    }
}

fn get_dir_size(path: PathBuf) -> u64 {
    let mut total_size = 0;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries {
            if let Ok(entry) = entry {
                if let Ok(metadata) = entry.metadata() {
                    if metadata.is_dir() {
                        total_size += get_dir_size(entry.path());
                    } else {
                        total_size += metadata.len();
                    }
                }
            }
        }
    }
    total_size
}

fn format_size(bytes: u64) -> String {
    let mb = bytes as f64 / 1024.0 / 1024.0;
    format!("{:.1} MB", mb)
}
