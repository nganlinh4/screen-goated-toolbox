use super::types::{CookieBrowser, DownloadState, DownloadType, InstallStatus};
use crate::gui::locale::LocaleText;
use eframe::egui;
use std::path::PathBuf;
use std::sync::atomic::Ordering;

use super::DownloadManager;

impl DownloadManager {
    fn has_deno_runtime(&self) -> bool {
        self.bin_dir.join("deno.exe").exists()
            || matches!(*self.deno_status.lock().unwrap(), InstallStatus::Installed)
    }

    fn set_cookie_browser_with_deno_guard(&mut self, browser: CookieBrowser) {
        if browser == self.cookie_browser {
            return;
        }

        if browser == CookieBrowser::None {
            self.cookie_browser = CookieBrowser::None;
            self.pending_cookie_browser = None;
            self.show_cookie_deno_dialog = false;
            self.save_settings();
            return;
        }

        if self.has_deno_runtime() {
            self.cookie_browser = browser;
            self.pending_cookie_browser = None;
            self.show_cookie_deno_dialog = false;
            self.save_settings();
            return;
        }

        self.pending_cookie_browser = Some(browser);
        self.show_cookie_deno_dialog = true;
        self.cookie_browser = CookieBrowser::None;
        self.save_settings();
    }

    fn apply_pending_cookie_browser_if_ready(&mut self) {
        if !self.show_cookie_deno_dialog || !self.has_deno_runtime() {
            return;
        }

        if let Some(browser) = self.pending_cookie_browser.take() {
            self.cookie_browser = browser;
            self.save_settings();
        }

        self.show_cookie_deno_dialog = false;
    }

    fn reject_cookie_browser_choice(&mut self) {
        self.cookie_browser = CookieBrowser::None;
        self.pending_cookie_browser = None;
        self.show_cookie_deno_dialog = false;
        self.save_settings();
    }

    pub fn render(&mut self, ctx: &egui::Context, text: &LocaleText) {
        if !self.show_window {
            // Reset focus state for all sessions when window is hidden
            for s in &mut self.sessions {
                s.initial_focus_set = false;
            }
            self.show_cookie_deno_dialog = false;
            self.pending_cookie_browser = None;
            return;
        }

        self.apply_pending_cookie_browser_if_ready();

        if self.cookie_browser != CookieBrowser::None && !self.has_deno_runtime() {
            self.pending_cookie_browser = Some(self.cookie_browser.clone());
            self.cookie_browser = CookieBrowser::None;
            self.show_cookie_deno_dialog = true;
            self.save_settings();
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
                                        self.install_cancel_flag.store(true, Ordering::Relaxed);
                                    }
                                }
                                InstallStatus::Extracting => {
                                    ui.label(text.download_status_extracting);
                                    ui.spinner();
                                    if ui.button(text.download_cancel_btn).clicked() {
                                        self.install_cancel_flag.store(true, Ordering::Relaxed);
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
                                        self.install_cancel_flag.store(true, Ordering::Relaxed);
                                    }
                                }
                                InstallStatus::Extracting => {
                                    ui.label(text.download_status_extracting);
                                    ui.spinner();
                                    if ui.button(text.download_cancel_btn).clicked() {
                                        self.install_cancel_flag.store(true, Ordering::Relaxed);
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
                        // --- TAB STRIP ---
                        ui.horizontal(|ui| {
                            let mut close_tab_idx: Option<usize> = None;
                            let mut switch_tab_idx: Option<usize> = None;
                            for i in 0..self.sessions.len() {
                                let is_active = i == self.active_tab_idx;
                                let label = self.sessions[i].tab_name.clone();
                                let tab_btn =
                                    egui::Button::new(egui::RichText::new(&label).size(11.0))
                                        .selected(is_active);
                                if ui.add(tab_btn).clicked() {
                                    switch_tab_idx = Some(i);
                                }
                                // Close button for this tab (×)
                                if ui.small_button("×").clicked() {
                                    close_tab_idx = Some(i);
                                }
                                ui.add_space(2.0);
                            }
                            if ui.small_button("+").on_hover_text("New tab").clicked() {
                                self.add_tab();
                            }
                            if let Some(idx) = switch_tab_idx {
                                self.active_tab_idx = idx;
                            }
                            if let Some(idx) = close_tab_idx {
                                self.close_tab(idx);
                            }
                        });
                        ui.separator();

                        let idx = self.active_idx();

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
                                        let (ytdlp_size, ffmpeg_size, deno_size) =
                                            self.get_dependency_sizes();
                                        let del_btn_text = text
                                            .download_delete_deps_btn
                                            .replacen("{}", &ytdlp_size, 1)
                                            .replacen("{}", &ffmpeg_size, 1)
                                            .replacen("{}", &deno_size, 1);

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
                            egui::TextEdit::singleline(&mut self.sessions[idx].input_url)
                                .hint_text("https://youtube.com/watch?v=...")
                                .desired_width(f32::INFINITY),
                        );

                        // Focus on first open
                        if !self.sessions[idx].initial_focus_set {
                            response.request_focus();
                            self.sessions[idx].initial_focus_set = true;
                        }

                        if response.changed() {
                            self.sessions[idx].last_input_change = ctx.input(|i| i.time);
                            self.sessions[idx].available_formats.lock().unwrap().clear();
                            self.sessions[idx].selected_format = None;
                        }

                        // Auto-analyze Logic
                        let time_since_edit =
                            ctx.input(|i| i.time) - self.sessions[idx].last_input_change;
                        let is_analyzing = *self.sessions[idx].is_analyzing.lock().unwrap();
                        let url_changed = self.sessions[idx].input_url.trim()
                            != self.sessions[idx].last_url_analyzed;

                        // Trigger analysis
                        if time_since_edit > 0.8
                            && url_changed
                            && !self.sessions[idx].input_url.trim().is_empty()
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
                                .radio_value(
                                    &mut self.sessions[idx].download_type,
                                    DownloadType::Video,
                                    "Video",
                                )
                                .changed()
                            {
                                self.save_settings();
                            }
                            if ui
                                .radio_value(
                                    &mut self.sessions[idx].download_type,
                                    DownloadType::Audio,
                                    "Audio",
                                )
                                .changed()
                            {
                                self.save_settings();
                            }

                            // Spacer
                            ui.add_space(10.0);

                            // Quality UI
                            if self.sessions[idx].download_type == DownloadType::Video {
                                let formats =
                                    self.sessions[idx].available_formats.lock().unwrap().clone();
                                let error =
                                    self.sessions[idx].analysis_error.lock().unwrap().clone();

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
                                    let current_val = self.sessions[idx]
                                        .selected_format
                                        .clone()
                                        .unwrap_or_else(|| best_text.clone());

                                    egui::ComboBox::from_id_salt("quality_combo")
                                        .selected_text(&current_val)
                                        .width(100.0) // Keep it compact
                                        .show_ui(ui, |ui| {
                                            ui.selectable_value(
                                                &mut self.sessions[idx].selected_format,
                                                None,
                                                &best_text,
                                            );
                                            for fmt in formats {
                                                ui.selectable_value(
                                                    &mut self.sessions[idx].selected_format,
                                                    Some(fmt.clone()),
                                                    &fmt,
                                                );
                                            }
                                        });

                                    // Subtitle Selection
                                    {
                                        let use_sub = *self.use_subtitles.lock().unwrap();
                                        if use_sub {
                                            let manual_subs = self.sessions[idx]
                                                .available_subs_manual
                                                .lock()
                                                .unwrap()
                                                .clone();

                                            if !manual_subs.is_empty() {
                                                ui.add_space(8.0);
                                                ui.label(text.download_subtitle_label);
                                                let auto_text =
                                                    text.download_subtitle_auto.to_string();
                                                let current_sub = self.sessions[idx]
                                                    .selected_subtitle
                                                    .clone()
                                                    .unwrap_or_else(|| auto_text.clone());

                                                egui::ComboBox::from_id_salt("subtitle_combo")
                                                    .selected_text(&current_sub)
                                                    .width(70.0)
                                                    .show_ui(ui, |ui| {
                                                        ui.label(
                                                            egui::RichText::new(
                                                                text.download_subs_found_header,
                                                            )
                                                            .small()
                                                            .weak(),
                                                        );
                                                        ui.separator();

                                                        if ui
                                                            .selectable_value(
                                                                &mut self.sessions[idx]
                                                                    .selected_subtitle,
                                                                None,
                                                                &auto_text,
                                                            )
                                                            .clicked()
                                                        {
                                                            self.save_settings();
                                                        }

                                                        for sub in manual_subs {
                                                            if ui
                                                                .selectable_label(
                                                                    self.sessions[idx]
                                                                        .selected_subtitle
                                                                        == Some(sub.clone()),
                                                                    &sub,
                                                                )
                                                                .clicked()
                                                            {
                                                                self.sessions[idx]
                                                                    .selected_subtitle =
                                                                    Some(sub.clone());
                                                                self.save_settings();
                                                            }
                                                        }
                                                    });
                                            } else {
                                                // No manual subs found
                                                ui.add_space(8.0);
                                                ui.colored_label(
                                                    egui::Color32::GRAY,
                                                    egui::RichText::new(
                                                        text.download_subs_none_found,
                                                    )
                                                    .small()
                                                    .italics(),
                                                );
                                            }
                                        }
                                    }
                                } else if error.is_some() {
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

                                        {
                                            let mut use_sub = self.use_subtitles.lock().unwrap();
                                            if ui
                                                .checkbox(&mut use_sub, text.download_opt_subtitles)
                                                .changed()
                                            {
                                                drop(use_sub);
                                                self.save_settings();
                                            }
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
                                            let mut selected_browser = None;
                                            for browser in &self.available_browsers {
                                                let label = match browser {
                                                    CookieBrowser::None => {
                                                        text.download_no_cookie_option.to_string()
                                                    }
                                                    other => other.to_string(),
                                                };
                                                if ui
                                                    .selectable_label(
                                                        self.cookie_browser == *browser,
                                                        label,
                                                    )
                                                    .clicked()
                                                {
                                                    selected_browser = Some(browser.clone());
                                                }
                                            }

                                            if let Some(browser) = selected_browser {
                                                self.set_cookie_browser_with_deno_guard(browser);
                                            }
                                        });
                                });
                            },
                        );

                        ui.add_space(15.0);

                        // --- ACTION AREA ---
                        // Define common button logic
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
                            .min_size(egui::vec2(ui.available_width(), 36.0)) // Slightly smaller height
                            .fill(btn_color);
                            ui.add(btn).clicked()
                        };

                        match &state {
                            DownloadState::Idle | DownloadState::Error(_) => {
                                if draw_download_btn(ui) && !self.sessions[idx].input_url.is_empty()
                                {
                                    // Reset logs on new start
                                    self.sessions[idx].logs.lock().unwrap().clear();
                                    self.sessions[idx].show_error_log = false;
                                    self.start_media_download(
                                        text.download_progress_info_fmt.to_string(),
                                    );
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
                                    let btn_text = if self.sessions[idx].show_error_log {
                                        text.download_hide_log_btn
                                    } else {
                                        text.download_show_log_btn
                                    };

                                    if ui
                                        .button(egui::RichText::new(btn_text).size(10.0))
                                        .clicked()
                                    {
                                        self.sessions[idx].show_error_log =
                                            !self.sessions[idx].show_error_log;
                                    }

                                    // Show Log Area
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
                                            let _ = open::that(path);
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
                                                let _ = open::that(path);
                                            }
                                        }
                                    });

                                    ui.add_space(8.0);
                                });

                                // Consistent Download Button at bottom
                                if draw_download_btn(ui) && !self.sessions[idx].input_url.is_empty()
                                {
                                    self.start_media_download(
                                        text.download_progress_info_fmt.to_string(),
                                    );
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

        if self.show_cookie_deno_dialog {
            self.apply_pending_cookie_browser_if_ready();
        }

        if self.show_cookie_deno_dialog {
            let deno_status = self.deno_status.lock().unwrap().clone();

            egui::Window::new(text.download_deno_required_title)
                .collapsible(false)
                .resizable(false)
                .fixed_size(egui::vec2(420.0, 180.0))
                .pivot(egui::Align2::CENTER_CENTER)
                .default_pos(ctx.input(|i| i.viewport_rect()).center())
                .show(ctx, |ui| {
                    ui.label(text.download_deno_required_body);
                    ui.label(text.download_deno_required_question);
                    ui.add_space(8.0);

                    match deno_status {
                        InstallStatus::Downloading(p) => {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(
                                    text.download_deno_downloading_fmt
                                        .replace("{}", &format!("{:.0}", p * 100.0)),
                                );
                            });
                            ui.add(egui::ProgressBar::new(p).desired_width(360.0));
                        }
                        InstallStatus::Extracting => {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(text.download_deno_extracting);
                            });
                        }
                        InstallStatus::Error(ref err) => {
                            ui.colored_label(
                                egui::Color32::RED,
                                text.download_deno_failed_fmt.replace("{}", err),
                            );
                        }
                        _ => {}
                    }

                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        let can_click_yes = !matches!(
                            deno_status,
                            InstallStatus::Downloading(_) | InstallStatus::Extracting
                        );
                        if ui
                            .add_enabled(
                                can_click_yes,
                                egui::Button::new(text.download_deno_yes_btn),
                            )
                            .clicked()
                        {
                            self.start_download_deno();
                        }
                        if ui.button(text.download_deno_no_btn).clicked() {
                            self.reject_cookie_browser_choice();
                        }
                    });
                });
        }

        self.show_window = open;
    }
}
