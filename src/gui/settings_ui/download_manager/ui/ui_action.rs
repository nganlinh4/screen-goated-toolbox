// Download action area and result rendering (error, finished, downloading states).

use super::super::types::DownloadState;
use crate::gui::locale::LocaleText;
use eframe::egui;

use super::super::DownloadManager;

impl DownloadManager {
    pub(super) fn render_action_area(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        text: &LocaleText,
        idx: usize,
    ) {
        let state = self.sessions[idx].download_state.lock().unwrap().clone();
        let is_analyzing = *self.sessions[idx].is_analyzing.lock().unwrap();

        let (btn_text, btn_color) = if is_analyzing {
            (
                text.download_scan_ignore_btn,
                egui::Color32::from_rgb(200, 100, 0),
            )
        } else {
            (
                text.download_start_btn,
                egui::Color32::from_rgb(0, 100, 200),
            )
        };

        let draw_download_btn = |ui: &mut egui::Ui| {
            let btn = egui::Button::new(
                egui::RichText::new(btn_text)
                    .heading()
                    .color(egui::Color32::WHITE),
            )
            .min_size(egui::vec2(ui.available_width(), 36.0))
            .fill(btn_color);
            ui.add(btn).clicked()
        };

        match &state {
            DownloadState::Idle | DownloadState::Error(_) => {
                if draw_download_btn(ui) && !self.sessions[idx].input_url.is_empty() {
                    self.sessions[idx].logs.lock().unwrap().clear();
                    self.sessions[idx].show_error_log = false;
                    self.start_media_download(text.download_progress_info_fmt.to_string());
                }
                if let DownloadState::Error(err) = &state {
                    self.render_error_state(ui, ctx, text, idx, err);
                }
            }
            DownloadState::Finished(path, _msg) => {
                self.render_finished_state(ui, ctx, text, path);
                if draw_download_btn(ui) && !self.sessions[idx].input_url.is_empty() {
                    self.start_media_download(text.download_progress_info_fmt.to_string());
                }
            }
            DownloadState::Downloading(progress, msg) => {
                ui.vertical_centered(|ui| {
                    ui.add_space(10.0);
                    if msg == "Starting..." {
                        ui.label(text.download_status_starting);
                    } else {
                        let clean_msg = msg.replace("[download]", "").trim().to_string();
                        ui.label(egui::RichText::new(clean_msg).small());
                    }
                    ui.add_space(5.0);
                    ui.add(egui::ProgressBar::new(*progress).animate(true));
                });
            }
        }
    }

    fn render_error_state(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        text: &LocaleText,
        idx: usize,
        err: &str,
    ) {
        ui.add_space(5.0);
        ui.label(
            egui::RichText::new(format!("{} {}", text.download_status_error, err))
                .color(egui::Color32::RED)
                .small(),
        );

        let btn_text = if self.sessions[idx].show_error_log {
            text.download_hide_log_btn
        } else {
            text.download_show_log_btn
        };

        if ui
            .button(egui::RichText::new(btn_text).size(10.0))
            .clicked()
        {
            self.sessions[idx].show_error_log = !self.sessions[idx].show_error_log;
        }

        if self.sessions[idx].show_error_log {
            ui.add_space(4.0);
            egui::Frame::group(ui.style())
                .fill(if ctx.style().visuals.dark_mode {
                    egui::Color32::from_black_alpha(100)
                } else {
                    egui::Color32::from_gray(240)
                })
                .show(ui, |ui| {
                    let logs = self.sessions[idx].logs.lock().unwrap();
                    let mut full_log_str = logs.join("\n");
                    egui::ScrollArea::vertical()
                        .max_height(120.0)
                        .show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut full_log_str)
                                    .font(egui::FontId::monospace(10.0))
                                    .desired_width(f32::INFINITY)
                                    .interactive(true)
                                    .lock_focus(false),
                            );
                        });
                });
        }
    }

    fn render_finished_state(
        &self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        text: &LocaleText,
        path: &std::path::Path,
    ) {
        ui.vertical_centered(|ui| {
            let success_color = if ctx.style().visuals.dark_mode {
                egui::Color32::GREEN
            } else {
                egui::Color32::from_rgb(0, 128, 0)
            };

            ui.label(
                egui::RichText::new(text.download_status_finished)
                    .color(success_color)
                    .heading(),
            );

            if let Some(name) = path.file_name() {
                let display_name = name
                    .to_string_lossy()
                    .replace("\u{29F8}", "/")
                    .replace("\u{FF0F}", "/")
                    .replace("\u{FF1A}", ":")
                    .replace("\u{FF1F}", "?")
                    .replace("\u{FF0A}", "*")
                    .replace("\u{FF1C}", "<")
                    .replace("\u{FF1E}", ">")
                    .replace("\u{FF5C}", "|")
                    .replace("\u{FF02}", "\"");
                ui.label(egui::RichText::new(display_name).small());
            }

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let enabled = path.components().next().is_some();
                if ui
                    .add_enabled(enabled, egui::Button::new(text.download_open_file_btn))
                    .clicked()
                {
                    let _ = open::that(path);
                }
                if ui
                    .add_enabled(enabled, egui::Button::new(text.download_open_folder_btn))
                    .clicked()
                {
                    if let Some(parent) = path.parent() {
                        let _ = open::that(parent);
                    } else {
                        let _ = open::that(path);
                    }
                }
            });

            ui.add_space(8.0);
        });
    }
}
