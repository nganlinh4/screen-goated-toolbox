use crate::api::realtime_audio::model_loader::{
    current_parakeet_model_notice, download_parakeet_model, get_parakeet_model_dir,
    is_model_downloaded, remove_parakeet_model,
};
use crate::api::realtime_audio::qwen3::assets::{
    current_qwen3_model_notice, download_qwen3_model, get_qwen3_model_dir,
    is_qwen3_model_downloaded, remove_qwen3_model,
};
use crate::api::realtime_audio::qwen3::server::{
    current_qwen3_server_notice, download_qwen3_server, get_active_qwen3_server_root,
    get_qwen3_server_dir, is_qwen3_server_downloaded, is_qwen3_server_managed,
    remove_qwen3_server,
};
use crate::gui::locale::LocaleText;
use crate::gui::settings_ui::download_manager::{DownloadManager, InstallStatus, UpdateStatus};
use crate::overlay::realtime_webview::state::REALTIME_STATE;
use eframe::egui;
use std::fs;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::thread;
mod ai_runtime;
mod backgrounds;
mod pointer_packs;
mod utils;
use self::{
    ai_runtime::render_ai_runtime_section,
    backgrounds::render_background_downloads_section,
    pointer_packs::render_pointer_pack_downloads_section,
    utils::{format_size, get_dir_size},
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
            .resizable(false)
            .default_width(650.0)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.add_space(8.0);
                render_ai_runtime_section(ui, text);
                ui.add_space(8.0);

                ui.group(|ui| {
                    let parakeet_notice = current_parakeet_model_notice();
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
                                    let _ = remove_parakeet_model();
                                }
                                let size = get_dir_size(&get_parakeet_model_dir());
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
                    if let Some(message) = parakeet_notice {
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new(message).color(egui::Color32::RED));
                    }
                });

                ui.add_space(8.0);

                ui.group(|ui| {
                    let qwen_notice = current_qwen3_model_notice();
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(text.tool_qwen3).strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let is_downloading = {
                                if let Ok(state) = REALTIME_STATE.lock() {
                                    state.is_downloading
                                        && state.download_title == text.qwen3_downloading_title
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
                            } else if is_qwen3_model_downloaded() {
                                if ui
                                    .button(
                                        egui::RichText::new(text.tool_action_delete)
                                            .color(egui::Color32::RED),
                                    )
                                    .clicked()
                                {
                                    let _ = remove_qwen3_model();
                                }
                                let size = get_dir_size(&get_qwen3_model_dir());
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
                                        let _ = download_qwen3_model(stop_signal, false);
                                    });
                                }
                                ui.label(
                                    egui::RichText::new(text.tool_status_missing)
                                        .color(egui::Color32::GRAY),
                                );
                            }
                        });
                    });
                    ui.label(text.tool_desc_qwen3);

                    if let Some(message) = qwen_notice {
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new(message).color(egui::Color32::RED));
                    }
                });

                ui.add_space(8.0);

                ui.group(|ui| {
                    let qwen_server_notice = current_qwen3_server_notice();
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(text.tool_qwen3_server).strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let is_downloading = {
                                if let Ok(state) = REALTIME_STATE.lock() {
                                    state.is_downloading
                                        && state.download_title
                                            == text.qwen3_server_downloading_title
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
                            } else if is_qwen3_server_downloaded() {
                                if is_qwen3_server_managed()
                                    && ui
                                        .button(
                                            egui::RichText::new(text.tool_action_delete)
                                                .color(egui::Color32::RED),
                                        )
                                        .clicked()
                                {
                                    let _ = remove_qwen3_server();
                                }
                                let size = get_active_qwen3_server_root()
                                    .map(|path| get_dir_size(&path))
                                    .unwrap_or_else(|| get_dir_size(&get_qwen3_server_dir()));
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
                                        let _ = download_qwen3_server(stop_signal, false);
                                    });
                                }
                                ui.label(
                                    egui::RichText::new(text.tool_status_missing)
                                        .color(egui::Color32::GRAY),
                                );
                            }
                        });
                    });
                    ui.label(text.tool_desc_qwen3_server);

                    if let Some(message) = qwen_server_notice {
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new(message).color(egui::Color32::RED));
                    }
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
                                    if let Ok(guard) = download_manager.ytdlp_version.lock()
                                        && let Some(ver) = &*guard
                                    {
                                        ui.label(
                                            egui::RichText::new(format!("v{}", ver))
                                                .color(egui::Color32::GRAY),
                                        );
                                    }
                                },
                            );
                        }
                    });
                });

                ui.add_space(8.0);

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
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            match status {
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
                                        *download_manager.deno_status.lock().unwrap() =
                                            InstallStatus::Missing;
                                    }

                                    let size = if let Ok(meta) =
                                        fs::metadata(download_manager.bin_dir.join("deno.exe"))
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
                                        download_manager.start_download_deno();
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
                        ui.label(text.tool_desc_deno);
                        if matches!(status, InstallStatus::Installed) {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
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
                                                        text.tool_update_available
                                                            .replace("{}", &ver),
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

                                    if let Ok(guard) = download_manager.deno_version.lock()
                                        && let Some(ver) = &*guard
                                    {
                                        ui.label(
                                            egui::RichText::new(format!("v{}", ver))
                                                .color(egui::Color32::GRAY),
                                        );
                                    }
                                },
                            );
                        }
                    });
                });

                ui.add_space(8.0);
                render_background_downloads_section(ui, text);
                ui.add_space(8.0);
                render_pointer_pack_downloads_section(ui, text);
                ui.add_space(8.0);

                ui.group(|ui| {
                    let ffmpeg_exists = download_manager.bin_dir.join("ffmpeg.exe").exists();
                    let ffprobe_exists = download_manager.bin_dir.join("ffprobe.exe").exists();
                    let installed_on_disk = ffmpeg_exists && ffprobe_exists;

                    // Keep UI status in sync when ffmpeg is installed externally
                    // (e.g., from the screen recorder panel).
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
                                    if let Ok(guard) = download_manager.ffmpeg_version.lock()
                                        && let Some(ver) = &*guard
                                    {
                                        ui.label(
                                            egui::RichText::new(format!("v{}", ver))
                                                .color(egui::Color32::GRAY),
                                        );
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
