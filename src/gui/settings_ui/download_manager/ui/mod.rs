use super::types::{CookieBrowser, InstallStatus};
use crate::gui::locale::LocaleText;
use crate::gui::theme::AppTheme;
use eframe::egui;

mod ui_action;
mod ui_main;

use super::DownloadManager;
use super::utils::has_nonempty_file;

impl DownloadManager {
    fn has_deno_runtime(&self) -> bool {
        has_nonempty_file(&self.bin_dir.join("deno.exe"))
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

        let theme = AppTheme::from_dark(ctx.global_style().visuals.dark_mode);
        let mut close_requested = false;

        // Fixed-size dialog -> Material modal with a scrim + dialog frame and a
        // shared dialog_header (title + inline folder/⚙ actions + close),
        // replacing the old egui::Window chrome while preserving the
        // open/close flag semantics.
        let modal = egui::Modal::new(egui::Id::new("download_manager_modal"))
            .backdrop_color(theme.scrim_color())
            .frame(theme.dialog_frame())
            .show(ctx, |ui| {
                ui.set_width(400.0);

                let ffmpeg_ok = matches!(
                    *self.ffmpeg_status.lock().unwrap(),
                    InstallStatus::Installed
                );
                let ytdlp_ok =
                    matches!(*self.ytdlp_status.lock().unwrap(), InstallStatus::Installed);

                // The destination-folder path + ⚙ settings menu live inline in
                // the title bar, but only once dependencies are ready (they
                // were previously part of the main UI body).
                if crate::gui::widgets::dialog_header(
                    ui,
                    &theme,
                    text.auxiliary.download.download_feature_title,
                    None,
                    |ui| {
                        if ffmpeg_ok && ytdlp_ok {
                            self.render_folder_settings(ui, ctx, text);
                        }
                    },
                ) {
                    close_requested = true;
                }

                if !ffmpeg_ok || !ytdlp_ok {
                    self.render_deps_check(ui, text);
                } else {
                    self.render_main_ui(ui, ctx, text);
                }
            });

        // Backdrop click / Escape also dismiss the dialog.
        if modal.should_close() {
            close_requested = true;
        }

        if self.show_cookie_deno_dialog {
            self.apply_pending_cookie_browser_if_ready();
        }

        if self.show_cookie_deno_dialog {
            self.render_deno_dialog(ctx, text);
        }

        if close_requested {
            self.show_window = false;
        }
    }

    fn render_deno_dialog(&mut self, ctx: &egui::Context, text: &LocaleText) {
        let theme = AppTheme::from_dark(ctx.global_style().visuals.dark_mode);
        let deno_status = self.deno_status.lock().unwrap().clone();

        egui::Window::new(text.auxiliary.download.download_deno_required_title)
            .collapsible(false)
            .resizable(false)
            .fixed_size(egui::vec2(420.0, 180.0))
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.input(|i| i.viewport_rect()).center())
            .show(ctx, |ui| {
                ui.label(text.auxiliary.download.download_deno_required_body);
                ui.label(text.auxiliary.download.download_deno_required_question);
                ui.add_space(8.0);

                match deno_status {
                    InstallStatus::Downloading(p) => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(
                                text.auxiliary
                                    .download
                                    .download_deno_downloading_fmt
                                    .replace("{}", &format!("{:.0}", p * 100.0)),
                            );
                        });
                        ui.add(egui::ProgressBar::new(p).desired_width(360.0));
                    }
                    InstallStatus::Extracting => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(text.auxiliary.download.download_deno_extracting);
                        });
                    }
                    InstallStatus::Error(ref err) => {
                        ui.colored_label(
                            theme.danger_text(),
                            text.auxiliary
                                .download
                                .download_deno_failed_fmt
                                .replace("{}", err),
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
                            egui::Button::new(text.auxiliary.download.download_deno_yes_btn),
                        )
                        .clicked()
                    {
                        self.start_download_deno();
                    }
                    if ui
                        .button(text.auxiliary.download.download_deno_no_btn)
                        .clicked()
                    {
                        self.reject_cookie_browser_choice();
                    }
                });
            });
    }
}
