use crate::gui::locale::LocaleText;
use crate::gui::settings_ui::download_manager::utils::has_nonempty_file;
use crate::gui::settings_ui::download_manager::{DownloadManager, InstallStatus, UpdateStatus};
use crate::gui::theme::AppTheme;
use eframe::egui;
use std::fs;

use super::utils::{format_size, tool_card};

pub(super) fn render_video_downloader_card(
    ui: &mut egui::Ui,
    download_manager: &mut DownloadManager,
    text: &LocaleText,
) {
    tool_card(ui, |ui| {
        ui.heading(text.tool_video_downloader_card);
        ui.add_space(4.0);

        render_ytdlp_content(ui, download_manager, text);
        ui.add_space(4.0);
        render_ffmpeg_content(ui, download_manager, text);
        ui.add_space(4.0);
        render_deno_content(ui, download_manager, text);
    });
}

fn render_ytdlp_content(
    ui: &mut egui::Ui,
    download_manager: &mut DownloadManager,
    text: &LocaleText,
) {
    let theme = AppTheme::from_ui(ui);
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
                            egui::RichText::new(text.tool_action_delete).color(theme.danger_text()),
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
                        .color(theme.success()),
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
                        egui::RichText::new(text.tool_status_missing).color(egui::Color32::GRAY),
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
                render_update_status(
                    ui,
                    u_status,
                    text,
                    || {
                        download_manager.start_download_ytdlp();
                    },
                    || {
                        download_manager.check_updates();
                    },
                );
                if let Ok(guard) = download_manager.ytdlp_version.lock()
                    && let Some(ver) = &*guard
                {
                    ui.label(egui::RichText::new(format!("v{}", ver)).color(egui::Color32::GRAY));
                }
            });
        }
    });
}

fn render_ffmpeg_content(
    ui: &mut egui::Ui,
    download_manager: &mut DownloadManager,
    text: &LocaleText,
) {
    let ffmpeg_exists = has_nonempty_file(&download_manager.bin_dir.join("ffmpeg.exe"));
    let ffprobe_exists = has_nonempty_file(&download_manager.bin_dir.join("ffprobe.exe"));
    let installed_on_disk = ffmpeg_exists && ffprobe_exists;

    {
        let mut s = download_manager.ffmpeg_status.lock().unwrap();
        let in_dl = matches!(
            *s,
            InstallStatus::Downloading(_) | InstallStatus::Extracting
        );
        if !in_dl {
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

    let theme = AppTheme::from_ui(ui);
    let status = download_manager.ffmpeg_status.lock().unwrap().clone();
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(text.tool_ffmpeg).strong());
        ui.with_layout(
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| match status {
                InstallStatus::Installed => {
                    if ui
                        .button(
                            egui::RichText::new(text.tool_action_delete).color(theme.danger_text()),
                        )
                        .clicked()
                    {
                        let _ = fs::remove_file(download_manager.bin_dir.join("ffmpeg.exe"));
                        let _ = fs::remove_file(download_manager.bin_dir.join("ffprobe.exe"));
                        let _ = fs::remove_file(
                            download_manager.bin_dir.join("ffmpeg_release_source.txt"),
                        );
                        *download_manager.ffmpeg_status.lock().unwrap() = InstallStatus::Missing;
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
                        .color(theme.success()),
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
                        egui::RichText::new(text.tool_status_missing).color(egui::Color32::GRAY),
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
                render_update_status(
                    ui,
                    u_status,
                    text,
                    || {
                        download_manager.start_download_ffmpeg();
                    },
                    || {
                        download_manager.check_updates();
                    },
                );
                if let Ok(guard) = download_manager.ffmpeg_version.lock()
                    && let Some(ver) = &*guard
                {
                    ui.label(egui::RichText::new(format!("v{}", ver)).color(egui::Color32::GRAY));
                }
            });
        }
    });
}

fn render_deno_content(
    ui: &mut egui::Ui,
    download_manager: &mut DownloadManager,
    text: &LocaleText,
) {
    let deno_exists = has_nonempty_file(&download_manager.bin_dir.join("deno.exe"));
    {
        let mut s = download_manager.deno_status.lock().unwrap();
        let in_dl = matches!(
            *s,
            InstallStatus::Downloading(_) | InstallStatus::Extracting
        );
        if !in_dl {
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

    let theme = AppTheme::from_ui(ui);
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
                            egui::RichText::new(text.tool_action_delete).color(theme.danger_text()),
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
                        .color(theme.success()),
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
                        egui::RichText::new(text.tool_status_missing).color(egui::Color32::GRAY),
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
                render_update_status(
                    ui,
                    u_status,
                    text,
                    || {
                        download_manager.start_download_deno();
                    },
                    || {
                        download_manager.check_updates();
                    },
                );
                if let Ok(guard) = download_manager.deno_version.lock()
                    && let Some(ver) = &*guard
                {
                    ui.label(egui::RichText::new(format!("v{}", ver)).color(egui::Color32::GRAY));
                }
            });
        }
    });
}

fn render_update_status(
    ui: &mut egui::Ui,
    status: UpdateStatus,
    text: &LocaleText,
    on_download: impl FnOnce(),
    on_check: impl FnOnce(),
) {
    let theme = AppTheme::from_ui(ui);
    match status {
        UpdateStatus::UpdateAvailable(ver) => {
            if ui
                .button(
                    egui::RichText::new(text.tool_update_available.replace("{}", &ver))
                        .color(theme.warning()),
                )
                .clicked()
            {
                on_download();
            }
        }
        UpdateStatus::Checking => {
            ui.spinner();
            ui.label(text.tool_update_checking);
        }
        UpdateStatus::UpToDate => {
            if ui.small_button(text.tool_update_check_again).clicked() {
                on_check();
            }
            ui.label(egui::RichText::new(text.tool_update_latest).color(theme.success()));
        }
        UpdateStatus::Error(e) => {
            if ui.small_button(text.tool_update_retry).clicked() {
                on_check();
            }
            ui.label(egui::RichText::new(text.tool_update_error).color(theme.danger_text()))
                .on_hover_text(e);
        }
        UpdateStatus::Idle => {
            if ui.small_button(text.tool_update_check_btn).clicked() {
                on_check();
            }
        }
    }
}
