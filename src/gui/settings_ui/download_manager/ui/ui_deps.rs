use std::sync::atomic::Ordering;

use eframe::egui;

use super::super::DownloadManager;
use super::super::types::InstallStatus;
use crate::gui::icons::{Icon, draw_icon_static};
use crate::gui::locale::LocaleText;
use crate::gui::theme::AppTheme;

impl DownloadManager {
    pub(super) fn render_deps_check(&mut self, ui: &mut egui::Ui, text: &LocaleText) {
        let theme = AppTheme::from_ui(ui);
        ui.label(text.auxiliary.download.download_deps_missing);
        self.render_dependency_row(ui, text, &theme, true);
        self.render_dependency_row(ui, text, &theme, false);
    }

    fn render_dependency_row(
        &self,
        ui: &mut egui::Ui,
        text: &LocaleText,
        theme: &AppTheme,
        yt_dlp: bool,
    ) {
        ui.group(|ui| {
            ui.horizontal(|ui| {
                let (label, status) = if yt_dlp {
                    (
                        text.auxiliary.download.download_deps_ytdlp,
                        self.ytdlp_status.lock().unwrap().clone(),
                    )
                } else {
                    (
                        text.auxiliary.download.download_deps_ffmpeg,
                        self.ffmpeg_status.lock().unwrap().clone(),
                    )
                };
                ui.label(label);
                match status {
                    InstallStatus::Checking => {
                        ui.spinner();
                        ui.label(text.auxiliary.download.download_status_checking);
                    }
                    InstallStatus::Missing | InstallStatus::Error(_) => {
                        if ui
                            .button(text.auxiliary.download.download_deps_download_btn)
                            .clicked()
                        {
                            if yt_dlp {
                                self.start_download_ytdlp();
                            } else {
                                self.start_download_ffmpeg();
                            }
                        }
                        if let InstallStatus::Error(error) = status {
                            ui.colored_label(theme.danger_text(), error);
                        }
                    }
                    InstallStatus::Unavailable => {
                        ui.colored_label(
                            theme.danger_text(),
                            text.auxiliary.managed_tools.tool_status_unavailable,
                        );
                    }
                    InstallStatus::Downloading(progress) => {
                        ui.label(format!("{:.0}%", progress * 100.0));
                        ui.add(egui::ProgressBar::new(progress).desired_width(120.0));
                        if ui
                            .button(text.auxiliary.download.download_cancel_btn)
                            .clicked()
                        {
                            self.install_cancel_flag.store(true, Ordering::Relaxed);
                        }
                    }
                    InstallStatus::Extracting => {
                        ui.label(text.auxiliary.download.download_status_extracting);
                        ui.spinner();
                        if ui
                            .button(text.auxiliary.download.download_cancel_btn)
                            .clicked()
                        {
                            self.install_cancel_flag.store(true, Ordering::Relaxed);
                        }
                    }
                    InstallStatus::Finalizing => {
                        ui.label(text.auxiliary.download.download_status_finalizing);
                        ui.spinner();
                    }
                    InstallStatus::Installed => {
                        draw_icon_static(ui, Icon::CheckCircle, Some(crate::gui::icons::ICON_MD));
                        ui.label(text.auxiliary.download.download_status_ready);
                    }
                }
            });
        });
    }
}
