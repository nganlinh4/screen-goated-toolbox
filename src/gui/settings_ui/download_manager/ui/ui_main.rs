// Main downloader UI panel (tab strip, URL input, format selection, action area).

use super::super::types::{DownloadType, InstallStatus};
use crate::gui::locale::LocaleText;
use eframe::egui;
use std::path::PathBuf;
use std::sync::atomic::Ordering;

use super::super::DownloadManager;

impl DownloadManager {
    /// Render the dependency check section when ffmpeg/yt-dlp are missing.
    pub(super) fn render_deps_check(&mut self, ui: &mut egui::Ui, text: &LocaleText) {
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
    }

    /// Render the main downloader UI (tabs, URL input, format, actions).
    pub(super) fn render_main_ui(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        text: &LocaleText,
    ) {
        egui::Frame::default().inner_margin(8.0).show(ui, |ui| {
            // --- TAB STRIP ---
            self.render_tab_strip(ui);
            ui.separator();

            let idx = self.active_idx();

            // --- FOLDER & SETTINGS ---
            self.render_folder_settings(ui, ctx, text);

            ui.add_space(8.0);

            // --- URL INPUT ---
            self.render_url_input(ui, ctx, text, idx);

            ui.add_space(8.0);

            // --- FORMAT & QUALITY ---
            self.render_format_quality(ui, text, idx);

            ui.add_space(8.0);

            // --- ADVANCED OPTIONS ---
            self.render_advanced_options(ui, text);

            ui.add_space(15.0);

            // --- ACTION AREA ---
            self.render_action_area(ui, ctx, text, idx);
        });
    }

    fn render_tab_strip(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let mut close_tab_idx: Option<usize> = None;
            let mut switch_tab_idx: Option<usize> = None;
            for i in 0..self.sessions.len() {
                let is_active = i == self.active_tab_idx;
                let label = self.sessions[i].tab_name.clone();
                let tab_btn =
                    egui::Button::new(egui::RichText::new(&label).size(11.0)).selected(is_active);
                if ui.add(tab_btn).clicked() {
                    switch_tab_idx = Some(i);
                }
                if ui.small_button("\u{00d7}").clicked() {
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
    }

    fn render_folder_settings(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        text: &LocaleText,
    ) {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("\u{1f4c2}").size(14.0));

            let current_path = self
                .custom_download_path
                .clone()
                .unwrap_or_else(|| dirs::download_dir().unwrap_or(PathBuf::from(".")));
            let path_str = current_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("...");

            ui.label(
                egui::RichText::new(format!("...\\{}", path_str))
                    .strong()
                    .color(ctx.style().visuals.weak_text_color()),
            );

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.menu_button("\u{2699}", |ui| {
                    if ui.button(text.download_change_folder_btn).clicked() {
                        self.change_download_folder();
                        ui.close();
                    }

                    ui.separator();

                    let (ytdlp_size, ffmpeg_size, deno_size) = self.get_dependency_sizes();
                    let del_btn_text = text
                        .download_delete_deps_btn
                        .replacen("{}", &ytdlp_size, 1)
                        .replacen("{}", &ffmpeg_size, 1)
                        .replacen("{}", &deno_size, 1);

                    if ui
                        .button(egui::RichText::new(del_btn_text).color(egui::Color32::RED))
                        .clicked()
                    {
                        self.delete_dependencies();
                        ui.close();
                    }
                });
            });
        });
    }

    fn render_url_input(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        text: &LocaleText,
        idx: usize,
    ) {
        ui.label(egui::RichText::new(text.download_url_label).strong());
        let response = ui.add(
            egui::TextEdit::singleline(&mut self.sessions[idx].input_url)
                .hint_text("https://youtube.com/watch?v=...")
                .desired_width(f32::INFINITY),
        );

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
        let time_since_edit = ctx.input(|i| i.time) - self.sessions[idx].last_input_change;
        let is_analyzing = *self.sessions[idx].is_analyzing.lock().unwrap();
        let url_changed =
            self.sessions[idx].input_url.trim() != self.sessions[idx].last_url_analyzed;

        if time_since_edit > 0.8
            && url_changed
            && !self.sessions[idx].input_url.trim().is_empty()
            && !is_analyzing
        {
            self.start_analysis();
        }
    }

    fn render_format_quality(&mut self, ui: &mut egui::Ui, text: &LocaleText, idx: usize) {
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

            ui.add_space(10.0);

            if self.sessions[idx].download_type == DownloadType::Video {
                self.render_video_quality(ui, text, idx);
            }
        });
    }

    fn render_video_quality(&mut self, ui: &mut egui::Ui, text: &LocaleText, idx: usize) {
        let formats = self.sessions[idx].available_formats.lock().unwrap().clone();
        let error = self.sessions[idx].analysis_error.lock().unwrap().clone();
        let is_analyzing = *self.sessions[idx].is_analyzing.lock().unwrap();

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
                .width(100.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.sessions[idx].selected_format, None, &best_text);
                    for fmt in formats {
                        ui.selectable_value(
                            &mut self.sessions[idx].selected_format,
                            Some(fmt.clone()),
                            &fmt,
                        );
                    }
                });

            // Subtitle Selection
            self.render_subtitle_selection(ui, text, idx);
        } else if error.is_some() {
            ui.colored_label(egui::Color32::RED, "\u{274c}");
        }
    }

    fn render_subtitle_selection(&mut self, ui: &mut egui::Ui, text: &LocaleText, idx: usize) {
        let use_sub = *self.use_subtitles.lock().unwrap();
        if !use_sub {
            return;
        }

        let manual_subs = self.sessions[idx]
            .available_subs_manual
            .lock()
            .unwrap()
            .clone();

        if !manual_subs.is_empty() {
            ui.add_space(8.0);
            ui.label(text.download_subtitle_label);
            let auto_text = text.download_subtitle_auto.to_string();
            let current_sub = self.sessions[idx]
                .selected_subtitle
                .clone()
                .unwrap_or_else(|| auto_text.clone());

            egui::ComboBox::from_id_salt("subtitle_combo")
                .selected_text(&current_sub)
                .width(70.0)
                .show_ui(ui, |ui| {
                    ui.label(
                        egui::RichText::new(text.download_subs_found_header)
                            .small()
                            .weak(),
                    );
                    ui.separator();

                    if ui
                        .selectable_value(
                            &mut self.sessions[idx].selected_subtitle,
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
                                self.sessions[idx].selected_subtitle == Some(sub.clone()),
                                &sub,
                            )
                            .clicked()
                        {
                            self.sessions[idx].selected_subtitle = Some(sub.clone());
                            self.save_settings();
                        }
                    }
                });
        } else {
            ui.add_space(8.0);
            ui.colored_label(
                egui::Color32::GRAY,
                egui::RichText::new(text.download_subs_none_found)
                    .small()
                    .italics(),
            );
        }
    }

    fn render_advanced_options(&mut self, ui: &mut egui::Ui, text: &LocaleText) {
        ui.collapsing(
            egui::RichText::new(text.download_advanced_header).strong(),
            |ui| {
                egui::Grid::new("adv_options_grid")
                    .num_columns(2)
                    .spacing([10.0, 4.0])
                    .show(ui, |ui| {
                        if ui
                            .checkbox(&mut self.use_metadata, text.download_opt_metadata)
                            .changed()
                        {
                            self.save_settings();
                        }
                        if ui
                            .checkbox(&mut self.use_sponsorblock, text.download_opt_sponsorblock)
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
                            .checkbox(&mut self.use_playlist, text.download_opt_playlist)
                            .changed()
                        {
                            self.save_settings();
                        }
                        ui.end_row();
                    });

                ui.add_space(4.0);
                self.render_cookie_browser_combo(ui, text);
            },
        );
    }

    fn render_cookie_browser_combo(&mut self, ui: &mut egui::Ui, text: &LocaleText) {
        ui.horizontal(|ui| {
            ui.label(text.download_opt_cookies);
            egui::ComboBox::from_id_salt("cookie_browser_combo")
                .selected_text(match &self.cookie_browser {
                    super::super::types::CookieBrowser::None => {
                        text.download_no_cookie_option.to_string()
                    }
                    other => other.to_string(),
                })
                .width(140.0)
                .show_ui(ui, |ui| {
                    let mut selected_browser = None;
                    for browser in &self.available_browsers {
                        let label = match browser {
                            super::super::types::CookieBrowser::None => {
                                text.download_no_cookie_option.to_string()
                            }
                            other => other.to_string(),
                        };
                        if ui
                            .selectable_label(self.cookie_browser == *browser, label)
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
    }
}
