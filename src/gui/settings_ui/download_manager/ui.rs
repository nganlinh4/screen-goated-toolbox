use super::types::{CookieBrowser, DownloadState, DownloadType, InstallStatus};
use crate::gui::locale::LocaleText;
use eframe::egui;
use std::path::PathBuf;

use super::DownloadManager;

impl DownloadManager {
    pub fn render(&mut self, ctx: &egui::Context, text: &LocaleText) {
        if !self.show_window {
            self.initial_focus_set = false;
            return;
        }

        let mut open = true;
        egui::Window::new(text.download_feature_title)
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(400.0)
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.input(|i| i.viewport_rect()).center())
            .show(ctx, |ui| {
                // Dependency Check
                let ffmpeg_ok = matches!(
                    *self.ffmpeg_status.lock().unwrap(),
                    InstallStatus::Installed
                );
                let ytdlp_ok =
                    matches!(*self.ytdlp_status.lock().unwrap(), InstallStatus::Installed);

                if !ffmpeg_ok || !ytdlp_ok {
                    ui.label(text.download_deps_missing);

                    // yt-dlp section
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(text.download_deps_ytdlp);
                            let status = self.ytdlp_status.lock().unwrap().clone();
                            match status {
                                InstallStatus::Checking => {
                                    ui.spinner();
                                }
                                InstallStatus::Missing | InstallStatus::Error(_) => {
                                    if ui.button(text.download_deps_download_btn).clicked() {
                                        self.start_download_ytdlp();
                                    }
                                    if let InstallStatus::Error(e) = status {
                                        ui.colored_label(egui::Color32::RED, e);
                                    }
                                }
                                InstallStatus::Downloading(p) => {
                                    ui.label(format!("{:.0}%", p * 100.0));
                                    ui.add(egui::ProgressBar::new(p).desired_width(120.0));
                                    if ui.button(text.download_cancel_btn).clicked() {
                                        self.cancel_download();
                                    }
                                }
                                InstallStatus::Extracting => {
                                    ui.label(text.download_status_extracting);
                                    ui.spinner();
                                    if ui.button(text.download_cancel_btn).clicked() {
                                        self.cancel_download();
                                    }
                                }
                                InstallStatus::Installed => {
                                    ui.label(text.download_status_ready);
                                }
                            }
                        });
                    });

                    // ffmpeg section
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(text.download_deps_ffmpeg);
                            let status = self.ffmpeg_status.lock().unwrap().clone();
                            match status {
                                InstallStatus::Checking => {
                                    ui.spinner();
                                }
                                InstallStatus::Missing | InstallStatus::Error(_) => {
                                    if ui.button(text.download_deps_download_btn).clicked() {
                                        self.start_download_ffmpeg();
                                    }
                                    if let InstallStatus::Error(e) = status {
                                        ui.colored_label(egui::Color32::RED, e);
                                    }
                                }
                                InstallStatus::Downloading(p) => {
                                    ui.label(format!("{:.0}%", p * 100.0));
                                    ui.add(egui::ProgressBar::new(p).desired_width(120.0));
                                    if ui.button(text.download_cancel_btn).clicked() {
                                        self.cancel_download();
                                    }
                                }
                                InstallStatus::Extracting => {
                                    ui.label(text.download_status_extracting);
                                    ui.spinner();
                                    if ui.button(text.download_cancel_btn).clicked() {
                                        self.cancel_download();
                                    }
                                }
                                InstallStatus::Installed => {
                                    ui.label(text.download_status_ready);
                                }
                            }
                        });
                    });
                } else {
                    // MAIN DOWNLOADER UI - COMPACT & NO SCROLLBAR
                    // Use a Frame with inner margin to keep things tidy but maximize space
                    egui::Frame::default().inner_margin(8.0).show(ui, |ui| {
                        // --- FOLDER & SETTINGS ---
                        ui.horizontal(|ui| {
                            // Compact Path:  📂 ...\Downloads  [⚙]
                            ui.label(egui::RichText::new("📂").size(14.0));

                            let current_path =
                                self.custom_download_path.clone().unwrap_or_else(|| {
                                    dirs::download_dir().unwrap_or(PathBuf::from("."))
                                });
                            let path_str = current_path
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("...");

                            // Truncate if too long (visual only)
                            ui.label(
                                egui::RichText::new(format!("...\\{}", path_str))
                                    .strong()
                                    .color(ctx.style().visuals.weak_text_color()),
                            );

                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.menu_button("⚙", |ui| {
                                        if ui.button(text.download_change_folder_btn).clicked() {
                                            self.change_download_folder();
                                            ui.close();
                                        }

                                        ui.separator();

                                        // Delete Dependencies
                                        let (ytdlp_size, ffmpeg_size) = self.get_dependency_sizes();
                                        let del_btn_text = text
                                            .download_delete_deps_btn
                                            .replacen("{}", &ytdlp_size, 1)
                                            .replacen("{}", &ffmpeg_size, 1);

                                        if ui
                                            .button(
                                                egui::RichText::new(del_btn_text)
                                                    .color(egui::Color32::RED),
                                            )
                                            .clicked()
                                        {
                                            self.delete_dependencies();
                                            ui.close();
                                        }
                                    });
                                },
                            );
                        });

                        ui.add_space(8.0);

                        // --- URL INPUT ---
                        // Compact Label + Input
                        ui.label(egui::RichText::new(text.download_url_label).strong());
                        let response = ui.add(
                            egui::TextEdit::singleline(&mut self.input_url)
                                .hint_text("https://youtube.com/watch?v=...")
                                .desired_width(f32::INFINITY),
                        );

                        // Focus on first open
                        if !self.initial_focus_set {
                            response.request_focus();
                            self.initial_focus_set = true;
                        }

                        if response.changed() {
                            self.last_input_change = ctx.input(|i| i.time);
                            self.available_formats.lock().unwrap().clear();
                            self.selected_format = None;
                        }

                        // Auto-analyze Logic
                        let time_since_edit = ctx.input(|i| i.time) - self.last_input_change;
                        let is_analyzing = *self.is_analyzing.lock().unwrap();
                        let url_changed = self.input_url.trim() != self.last_url_analyzed;

                        // Trigger analysis
                        if time_since_edit > 0.8
                            && url_changed
                            && !self.input_url.trim().is_empty()
                            && !is_analyzing
                        {
                            self.start_analysis();
                        }

                        ui.add_space(8.0);

                        // --- FORMAT & QUALITY (ONE LINE) ---
                        // [Radio Video] [Radio Audio] | [Quality: Best v] (or Spinner)
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(text.download_format_label).strong());
                            if ui
                                .radio_value(&mut self.download_type, DownloadType::Video, "Video")
                                .changed()
                            {
                                self.save_settings();
                            }
                            if ui
                                .radio_value(&mut self.download_type, DownloadType::Audio, "Audio")
                                .changed()
                            {
                                self.save_settings();
                            }

                            // Spacer
                            ui.add_space(10.0);

                            // Quality UI
                            if self.download_type == DownloadType::Video {
                                let formats = self.available_formats.lock().unwrap().clone();
                                let error = self.analysis_error.lock().unwrap().clone();

                                if is_analyzing {
                                    ui.spinner();
                                    ui.label(
                                        egui::RichText::new(text.download_scanning_label)
                                            .italics()
                                            .size(11.0),
                                    );
                                } else if !formats.is_empty() {
                                    ui.label(text.download_quality_label_text);
                                    let best_text = text.download_quality_best.to_string();
                                    let current_val = self
                                        .selected_format
                                        .clone()
                                        .unwrap_or_else(|| best_text.clone());

                                    egui::ComboBox::from_id_salt("quality_combo")
                                        .selected_text(&current_val)
                                        .width(100.0) // Keep it compact
                                        .show_ui(ui, |ui| {
                                            ui.selectable_value(
                                                &mut self.selected_format,
                                                None,
                                                &best_text,
                                            );
                                            for fmt in formats {
                                                ui.selectable_value(
                                                    &mut self.selected_format,
                                                    Some(fmt.clone()),
                                                    &fmt,
                                                );
                                            }
                                        });
                                } else if let Some(_) = error {
                                    // Error will be shown in status, just show generic fail here or nothing to keep compact
                                    ui.colored_label(egui::Color32::RED, "❌");
                                }
                            }
                        });

                        ui.add_space(8.0);

                        // --- ADVANCED OPTIONS (Compact) ---
                        ui.collapsing(
                            egui::RichText::new(text.download_advanced_header).strong(),
                            |ui| {
                                egui::Grid::new("adv_options_grid")
                                    .num_columns(2)
                                    .spacing([10.0, 4.0])
                                    .show(ui, |ui| {
                                        if ui
                                            .checkbox(
                                                &mut self.use_metadata,
                                                text.download_opt_metadata,
                                            )
                                            .changed()
                                        {
                                            self.save_settings();
                                        }
                                        if ui
                                            .checkbox(
                                                &mut self.use_sponsorblock,
                                                text.download_opt_sponsorblock,
                                            )
                                            .changed()
                                        {
                                            self.save_settings();
                                        }
                                        ui.end_row();

                                        if ui
                                            .checkbox(
                                                &mut self.use_subtitles,
                                                text.download_opt_subtitles,
                                            )
                                            .changed()
                                        {
                                            self.save_settings();
                                        }
                                        if ui
                                            .checkbox(
                                                &mut self.use_playlist,
                                                text.download_opt_playlist,
                                            )
                                            .changed()
                                        {
                                            self.save_settings();
                                        }
                                        ui.end_row();
                                    });

                                ui.add_space(4.0);
                                ui.horizontal(|ui| {
                                    ui.label(text.download_opt_cookies);
                                    egui::ComboBox::from_id_salt("cookie_browser_combo")
                                        .selected_text(match &self.cookie_browser {
                                            CookieBrowser::None => {
                                                text.download_no_cookie_option.to_string()
                                            }
                                            other => other.to_string(),
                                        })
                                        .width(140.0)
                                        .show_ui(ui, |ui| {
                                            for browser in &self.available_browsers {
                                                let label = match browser {
                                                    CookieBrowser::None => {
                                                        text.download_no_cookie_option.to_string()
                                                    }
                                                    other => other.to_string(),
                                                };
                                                if ui
                                                    .selectable_value(
                                                        &mut self.cookie_browser,
                                                        browser.clone(),
                                                        label,
                                                    )
                                                    .changed()
                                                {
                                                    self.save_settings();
                                                }
                                            }
                                        });
                                });
                            },
                        );

                        ui.add_space(15.0);

                        // --- ACTION AREA ---
                        // Define common button logic
                        let state = self.download_state.lock().unwrap().clone();
                        let is_analyzing = *self.is_analyzing.lock().unwrap();

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
                            .min_size(egui::vec2(ui.available_width(), 36.0)) // Slightly smaller height
                            .fill(btn_color);
                            ui.add(btn).clicked()
                        };

                        match &state {
                            DownloadState::Idle | DownloadState::Error(_) => {
                                if draw_download_btn(ui) {
                                    if !self.input_url.is_empty() {
                                        // Reset logs on new start
                                        self.logs.lock().unwrap().clear();
                                        self.show_error_log = false;
                                        self.start_media_download(
                                            text.download_progress_info_fmt.to_string(),
                                        );
                                    }
                                }
                                if let DownloadState::Error(err) = &state {
                                    ui.add_space(5.0);
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "{} {}",
                                            text.download_status_error, err
                                        ))
                                        .color(egui::Color32::RED)
                                        .small(),
                                    );

                                    // Toggle Log Button
                                    let btn_text = if self.show_error_log {
                                        text.download_hide_log_btn
                                    } else {
                                        text.download_show_log_btn
                                    };

                                    if ui
                                        .button(egui::RichText::new(btn_text).size(10.0))
                                        .clicked()
                                    {
                                        self.show_error_log = !self.show_error_log;
                                    }

                                    // Show Log Area
                                    if self.show_error_log {
                                        ui.add_space(4.0);
                                        egui::Frame::group(ui.style())
                                            .fill(if ctx.style().visuals.dark_mode {
                                                egui::Color32::from_black_alpha(100)
                                            } else {
                                                egui::Color32::from_gray(240)
                                            })
                                            .show(ui, |ui| {
                                                let logs = self.logs.lock().unwrap();
                                                let mut full_log_str = logs.join("\n");
                                                egui::ScrollArea::vertical()
                                                    .max_height(120.0)
                                                    .show(ui, |ui| {
                                                        ui.add(
                                                            egui::TextEdit::multiline(
                                                                &mut full_log_str,
                                                            )
                                                            .font(egui::FontId::monospace(10.0))
                                                            .desired_width(f32::INFINITY)
                                                            .interactive(true)
                                                            .lock_focus(false),
                                                        );
                                                    });
                                            });
                                    }
                                }
                            }
                            DownloadState::Finished(path, _msg) => {
                                // "Finished" View
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

                                    // Compact file info
                                    if let Some(name) = path.file_name() {
                                        let display_name = name
                                            .to_string_lossy()
                                            .replace("\u{29F8}", "/") // Big Solidus
                                            .replace("\u{FF0F}", "/") // Fullwidth Solidus
                                            .replace("\u{FF1A}", ":") // Fullwidth Colon
                                            .replace("\u{FF1F}", "?") // Fullwidth Question Mark
                                            .replace("\u{FF0A}", "*") // Fullwidth Asterisk
                                            .replace("\u{FF1C}", "<") // Fullwidth Less-Than
                                            .replace("\u{FF1E}", ">") // Fullwidth Greater-Than
                                            .replace("\u{FF5C}", "|") // Fullwidth Vertical Line
                                            .replace("\u{FF02}", "\""); // Fullwidth Quotation Mark
                                        ui.label(egui::RichText::new(display_name).small());
                                    }

                                    ui.add_space(4.0);
                                    ui.horizontal(|ui| {
                                        let enabled = path.components().next().is_some();
                                        if ui
                                            .add_enabled(
                                                enabled,
                                                egui::Button::new(text.download_open_file_btn),
                                            )
                                            .clicked()
                                        {
                                            let _ = open::that(&path);
                                        }
                                        if ui
                                            .add_enabled(
                                                enabled,
                                                egui::Button::new(text.download_open_folder_btn),
                                            )
                                            .clicked()
                                        {
                                            if let Some(parent) = path.parent() {
                                                let _ = open::that(parent);
                                            } else {
                                                let _ = open::that(&path);
                                            }
                                        }
                                    });

                                    ui.add_space(8.0);
                                });

                                // Consistent Download Button at bottom
                                if draw_download_btn(ui) {
                                    if !self.input_url.is_empty() {
                                        self.start_media_download(
                                            text.download_progress_info_fmt.to_string(),
                                        );
                                    }
                                }
                            }
                            DownloadState::Downloading(progress, msg) => {
                                ui.vertical_centered(|ui| {
                                    ui.add_space(10.0);
                                    if msg == "Starting..." {
                                        ui.label(text.download_status_starting);
                                    } else {
                                        let clean_msg =
                                            msg.replace("[download]", "").trim().to_string();
                                        ui.label(egui::RichText::new(clean_msg).small());
                                    }
                                    ui.add_space(5.0);
                                    ui.add(egui::ProgressBar::new(*progress).animate(true));
                                });
                            }
                        }
                    });
                }
            });

        self.show_window = open;
    }
}
