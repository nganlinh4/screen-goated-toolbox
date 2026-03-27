use super::types::{CookieBrowser, InstallStatus};
use crate::gui::locale::LocaleText;
use eframe::egui;

mod ui_action;
mod ui_main;

use super::DownloadManager;

impl DownloadManager {
    fn has_deno_runtime(&self) -> bool {
        self.bin_dir.join("deno.exe").exists()
            || matches!(*self.deno_status.lock().unwrap(), InstallStatus::Installed)
    }

    pub(super) fn set_cookie_browser_with_deno_guard(&mut self, browser: CookieBrowser) {
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
                let ffmpeg_ok = matches!(
                    *self.ffmpeg_status.lock().unwrap(),
                    InstallStatus::Installed
                );
                let ytdlp_ok =
                    matches!(*self.ytdlp_status.lock().unwrap(), InstallStatus::Installed);

                if !ffmpeg_ok || !ytdlp_ok {
                    self.render_deps_check(ui, text);
                } else {
                    self.render_main_ui(ui, ctx, text);
                }
            });

        if self.show_cookie_deno_dialog {
            self.apply_pending_cookie_browser_if_ready();
        }

        if self.show_cookie_deno_dialog {
            self.render_deno_dialog(ctx, text);
        }

        self.show_window = open;
    }

    fn render_deno_dialog(&mut self, ctx: &egui::Context, text: &LocaleText) {
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
                        .add_enabled(can_click_yes, egui::Button::new(text.download_deno_yes_btn))
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
}
