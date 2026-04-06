use crate::gui::locale::LocaleText;
use crate::gui::settings_ui::download_manager::{DownloadManager, InstallStatus, UpdateStatus};
use eframe::egui;
use std::fs;

mod ai_runtime;
mod backgrounds;
mod model_sections;
mod pointer_packs;
mod utils;
mod zipformer;

use self::{
    ai_runtime::render_ai_runtime_section,
    backgrounds::render_background_downloads_section,
    model_sections::{
        render_parakeet_section, render_qwen3_1_7b_section, render_qwen3_runtime_section,
        render_qwen3_section,
    },
    pointer_packs::render_pointer_pack_downloads_section,
    zipformer::render_zipformer_section,
};

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
            .resizable(true)
            .default_width(1280.0)
            .default_height(820.0)
            .min_width(1080.0)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_min_width(1080.0);
                egui::ScrollArea::vertical()
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        ui.add_space(8.0);

                        ui.columns(2, |columns| {
                            columns[0].vertical(|ui| {
                                render_ai_runtime_section(ui, text);
                                ui.add_space(8.0);
                                render_parakeet_section(ui, text);
                                ui.add_space(8.0);
                                render_qwen3_section(ui, text);
                                ui.add_space(8.0);
                                render_qwen3_1_7b_section(ui, text);
                                ui.add_space(8.0);
                                render_deno_section(ui, download_manager, text);
                                ui.add_space(8.0);
                                render_pointer_pack_downloads_section(ui, text);
                            });

                            columns[1].vertical(|ui| {
                                render_qwen3_runtime_section(ui, text);
                                ui.add_space(8.0);
                                render_background_downloads_section(ui, text);
                                ui.add_space(8.0);
                                render_ytdlp_section(ui, download_manager, text);
                                ui.add_space(8.0);
                                render_ffmpeg_section(ui, download_manager, text);
                                ui.add_space(8.0);
                                render_zipformer_section(ui);
                            });
                        });
                    });
            });

        *show_modal = open;
    }
}

fn render_ytdlp_section(
    ui: &mut egui::Ui,
    download_manager: &mut DownloadManager,
    text: &LocaleText,
) {
    ui.group(|ui| {
        let status = download_manager.ytdlp_status.lock().unwrap().clone();
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(text.tool_ytdlp).strong());
            ui.with_layout(
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| match status {
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
                            *download_manager.ytdlp_status.lock().unwrap() = InstallStatus::Missing;
                        }
                        let size = fs::metadata(download_manager.bin_dir.join("yt-dlp.exe"))
                            .map(|meta| meta.len())
                            .unwrap_or(0);
                        ui.label(
                            egui::RichText::new(
                                text.tool_status_installed.replace("{}", &format_size(size)),
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
                },
            );
        });

        ui.horizontal(|ui| {
            ui.label(text.tool_desc_ytdlp);
            if matches!(status, InstallStatus::Installed) {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
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
                                        text.tool_update_available.replace("{}", &ver),
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
                            if ui.small_button(text.tool_update_check_again).clicked() {
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
                            if ui.small_button(text.tool_update_check_btn).clicked() {
                                download_manager.check_updates();
                            }
                        }
                    }

                    if let Ok(guard) = download_manager.ytdlp_version.lock()
                        && let Some(ver) = &*guard
                    {
                        ui.label(
                            egui::RichText::new(format!("v{}", ver)).color(egui::Color32::GRAY),
                        );
                    }
                });
            }
        });
    });
}

fn render_deno_section(
    ui: &mut egui::Ui,
    download_manager: &mut DownloadManager,
    text: &LocaleText,
) {
    ui.group(|ui| {
        let deno_exists = download_manager.bin_dir.join("deno.exe").exists();

        {
            let mut s = download_manager.deno_status.lock().unwrap();
            let in_downloading_state = matches!(
                *s,
                InstallStatus::Downloading(_) | InstallStatus::Extracting
            );
            if !in_downloading_state {
                match (&*s, deno_exists) {
                    (InstallStatus::Installed, false) => *s = InstallStatus::Missing,
                    (InstallStatus::Missing, true) => *s = InstallStatus::Installed,
                    (InstallStatus::Checking, true) => *s = InstallStatus::Installed,
                    (InstallStatus::Checking, false) => *s = InstallStatus::Missing,
                    (InstallStatus::Error(_), true) => *s = InstallStatus::Installed,
                    _ => {}
                }
            }
        }

        let status = download_manager.deno_status.lock().unwrap().clone();
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(text.tool_deno).strong());
            ui.with_layout(
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| match status {
                    InstallStatus::Installed => {
                        let path = download_manager.bin_dir.join("deno.exe");
                        if ui
                            .button(
                                egui::RichText::new(text.tool_action_delete)
                                    .color(egui::Color32::RED),
                            )
                            .clicked()
                        {
                            let _ = fs::remove_file(path);
                            *download_manager.deno_status.lock().unwrap() = InstallStatus::Missing;
                        }
                        let size = fs::metadata(download_manager.bin_dir.join("deno.exe"))
                            .map(|meta| meta.len())
                            .unwrap_or(0);
                        ui.label(
                            egui::RichText::new(
                                text.tool_status_installed.replace("{}", &format_size(size)),
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
                            download_manager.start_download_deno();
                        }
                        ui.label(
                            egui::RichText::new(text.tool_status_missing)
                                .color(egui::Color32::GRAY),
                        );
                    }
                },
            );
        });

        ui.horizontal(|ui| {
            ui.label(text.tool_desc_deno);
            if matches!(status, InstallStatus::Installed) {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let u_status = {
                        if let Ok(s) = download_manager.deno_update_status.lock() {
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
                                        text.tool_update_available.replace("{}", &ver),
                                    )
                                    .color(egui::Color32::from_rgb(255, 165, 0)),
                                )
                                .clicked()
                            {
                                download_manager.start_download_deno();
                            }
                        }
                        UpdateStatus::Checking => {
                            ui.spinner();
                            ui.label(text.tool_update_checking);
                        }
                        UpdateStatus::UpToDate => {
                            if ui.small_button(text.tool_update_check_again).clicked() {
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
                            if ui.small_button(text.tool_update_check_btn).clicked() {
                                download_manager.check_updates();
                            }
                        }
                    }

                    if let Ok(guard) = download_manager.deno_version.lock()
                        && let Some(ver) = &*guard
                    {
                        ui.label(
                            egui::RichText::new(format!("v{}", ver)).color(egui::Color32::GRAY),
                        );
                    }
                });
            }
        });
    });
}

fn render_ffmpeg_section(
    ui: &mut egui::Ui,
    download_manager: &mut DownloadManager,
    text: &LocaleText,
) {
    ui.group(|ui| {
        let ffmpeg_exists = download_manager.bin_dir.join("ffmpeg.exe").exists();
        let ffprobe_exists = download_manager.bin_dir.join("ffprobe.exe").exists();
        let installed_on_disk = ffmpeg_exists && ffprobe_exists;

        {
            let mut s = download_manager.ffmpeg_status.lock().unwrap();
            let in_downloading_state = matches!(
                *s,
                InstallStatus::Downloading(_) | InstallStatus::Extracting
            );
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
            ui.with_layout(
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| match status {
                    InstallStatus::Installed => {
                        if ui
                            .button(
                                egui::RichText::new(text.tool_action_delete)
                                    .color(egui::Color32::RED),
                            )
                            .clicked()
                        {
                            let _ = fs::remove_file(download_manager.bin_dir.join("ffmpeg.exe"));
                            let _ = fs::remove_file(download_manager.bin_dir.join("ffprobe.exe"));
                            let _ = fs::remove_file(
                                download_manager.bin_dir.join("ffmpeg_release_source.txt"),
                            );
                            *download_manager.ffmpeg_status.lock().unwrap() =
                                InstallStatus::Missing;
                        }
                        let size = [
                            download_manager.bin_dir.join("ffmpeg.exe"),
                            download_manager.bin_dir.join("ffprobe.exe"),
                        ]
                        .into_iter()
                        .filter_map(|path| fs::metadata(path).ok())
                        .map(|meta| meta.len())
                        .sum::<u64>();
                        ui.label(
                            egui::RichText::new(
                                text.tool_status_installed.replace("{}", &format_size(size)),
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
                },
            );
        });

        ui.horizontal(|ui| {
            ui.label(text.tool_desc_ffmpeg);
            if matches!(status, InstallStatus::Installed) {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let u_status = {
                        if let Ok(s) = download_manager.ffmpeg_update_status.lock() {
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
                                        text.tool_update_available.replace("{}", &ver),
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
                            if ui.small_button(text.tool_update_check_again).clicked() {
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
                            if ui.small_button(text.tool_update_check_btn).clicked() {
                                download_manager.check_updates();
                            }
                        }
                    }

                    if let Ok(guard) = download_manager.ffmpeg_version.lock()
                        && let Some(ver) = &*guard
                    {
                        ui.label(
                            egui::RichText::new(format!("v{}", ver)).color(egui::Color32::GRAY),
                        );
                    }
                });
            }
        });
    });
}

fn format_size(bytes: u64) -> String {
    let mb = bytes as f64 / 1024.0 / 1024.0;
    format!("{:.1} MB", mb)
}
