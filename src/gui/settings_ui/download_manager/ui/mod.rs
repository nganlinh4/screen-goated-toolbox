use super::types::{CookieBrowser, DownloadState, InstallStatus};
use crate::gui::locale::LocaleText;
use crate::gui::theme::AppTheme;
use eframe::egui;

mod ui_action;
mod ui_deps;
mod ui_main;

use super::DownloadManager;

fn should_render_dependency_gate(
    ffmpeg_status: &InstallStatus,
    ytdlp_status: &InstallStatus,
    media_workflow_active: bool,
) -> bool {
    !media_workflow_active
        && (!matches!(ffmpeg_status, InstallStatus::Installed)
            || !matches!(ytdlp_status, InstallStatus::Installed))
}

/// The Deno offer appears only once Deno is known to be absent; a status that
/// is still being checked or installed says nothing about the saved choice.
fn cookie_browser_needs_deno(browser: &CookieBrowser, deno: &InstallStatus) -> bool {
    *browser != CookieBrowser::None && matches!(deno, InstallStatus::Missing)
}

impl DownloadManager {
    fn media_workflow_active(&self) -> bool {
        self.sessions.iter().any(|session| {
            matches!(
                *session.download_state.lock().unwrap(),
                DownloadState::Downloading(_, _)
            )
        })
    }

    fn has_deno_runtime(&self) -> bool {
        matches!(*self.deno_status.lock().unwrap(), InstallStatus::Installed)
    }

    fn saved_cookie_browser_needs_deno(&self) -> bool {
        !self.show_cookie_deno_dialog
            && cookie_browser_needs_deno(&self.cookie_browser, &self.deno_status.lock().unwrap())
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
            self.folder_picker.retire();
            for s in &mut self.sessions {
                s.initial_focus_set = false;
            }
            self.show_cookie_deno_dialog = false;
            self.pending_cookie_browser = None;
            return;
        }

        self.browser_discovery.update(ctx);
        self.poll_folder_picker(text);

        self.apply_pending_cookie_browser_if_ready();

        // Offer Deno without touching the saved choice: downloads that use
        // cookies prepare Deno themselves, and only rejecting the dialog
        // clears the preference.
        if self.saved_cookie_browser_needs_deno() {
            self.pending_cookie_browser = Some(self.cookie_browser.clone());
            self.show_cookie_deno_dialog = true;
        }

        let theme = AppTheme::from_dark(ctx.global_style().visuals.dark_mode);
        let mut close_requested = false;

        // Fixed-size dialog -> Material modal with a scrim + dialog frame and a
        // shared dialog_header (title + inline folder/⚙ actions + close),
        // replacing the old egui::Window chrome while preserving the
        // open/close flag semantics.
        let modal = crate::gui::widgets::material_modal(
            ctx,
            &theme,
            egui::Id::new("download_manager_modal"),
            |ui| {
                ui.set_width(400.0);

                let ffmpeg_status = self.ffmpeg_status.lock().unwrap().clone();
                let ytdlp_status = self.ytdlp_status.lock().unwrap().clone();
                let ffmpeg_ok = matches!(&ffmpeg_status, InstallStatus::Installed);
                let ytdlp_ok = matches!(&ytdlp_status, InstallStatus::Installed);
                let media_workflow_active = self.media_workflow_active();

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

                if should_render_dependency_gate(
                    &ffmpeg_status,
                    &ytdlp_status,
                    media_workflow_active,
                ) {
                    self.render_deps_check(ui, text);
                } else {
                    self.render_main_ui(ui, ctx, text);
                }
            },
        );

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
            self.folder_picker.retire();
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
                    InstallStatus::Finalizing => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(text.auxiliary.download.download_status_finalizing);
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
                    InstallStatus::Unavailable => {
                        ui.colored_label(
                            theme.danger_text(),
                            text.auxiliary.managed_tools.tool_status_unavailable,
                        );
                    }
                    InstallStatus::Checking => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(text.auxiliary.download.download_status_checking);
                        });
                    }
                    _ => {}
                }

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let can_click_yes = !matches!(
                        deno_status,
                        InstallStatus::Downloading(_)
                            | InstallStatus::Extracting
                            | InstallStatus::Finalizing
                            | InstallStatus::Unavailable
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

#[cfg(test)]
mod tests {
    use super::{cookie_browser_needs_deno, should_render_dependency_gate};
    use crate::gui::settings_ui::download_manager::types::{CookieBrowser, InstallStatus};

    #[test]
    fn saved_cookie_browser_survives_an_unsettled_deno_status() {
        for status in [
            InstallStatus::Checking,
            InstallStatus::Downloading(0.5),
            InstallStatus::Extracting,
            InstallStatus::Finalizing,
            InstallStatus::Installed,
            InstallStatus::Error("offline".to_string()),
        ] {
            assert!(
                !cookie_browser_needs_deno(&CookieBrowser::Chrome, &status),
                "{status:?}"
            );
        }
        assert!(cookie_browser_needs_deno(
            &CookieBrowser::Chrome,
            &InstallStatus::Missing
        ));
        assert!(!cookie_browser_needs_deno(
            &CookieBrowser::None,
            &InstallStatus::Missing
        ));
    }

    #[test]
    fn active_media_workflow_does_not_reopen_dependency_gate() {
        assert!(!should_render_dependency_gate(
            &InstallStatus::Downloading(0.0),
            &InstallStatus::Installed,
            true,
        ));
    }

    #[test]
    fn idle_missing_dependency_still_shows_dependency_gate() {
        assert!(should_render_dependency_gate(
            &InstallStatus::Missing,
            &InstallStatus::Installed,
            false,
        ));
    }
}
