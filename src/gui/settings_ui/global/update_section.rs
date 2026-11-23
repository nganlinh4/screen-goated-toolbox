use crate::gui::locale::LocaleText;
use crate::updater::{UpdateStatus, Updater};
use eframe::egui;

pub fn render_update_section_content(
    ui: &mut egui::Ui,
    updater: &Option<Updater>,
    status: &UpdateStatus,
    text: &LocaleText,
) {
    let theme = crate::gui::theme::AppTheme::from_ui(ui);
    match status {
        UpdateStatus::Idle => {
            ui.horizontal(|ui| {
                let ver_string = format!(
                    "{} v{}",
                    text.desktop_settings.current_version_label,
                    env!("CARGO_PKG_VERSION")
                );
                ui.label(ver_string);
                if ui
                    .button(text.desktop_settings.check_for_updates_btn)
                    .clicked()
                    && let Some(u) = updater
                {
                    u.check_for_updates();
                }
            });
        }
        UpdateStatus::Checking => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(text.desktop_settings.checking_github);
            });
        }
        UpdateStatus::UpToDate(ver) => {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("{} (v{})", text.desktop_settings.up_to_date, ver))
                        .color(theme.success()),
                );
                if ui.button(text.desktop_settings.check_again_btn).clicked()
                    && let Some(u) = updater
                {
                    u.check_for_updates();
                }
            });
        }
        UpdateStatus::UpdateAvailable { version, body } => {
            ui.colored_label(
                theme.warning(),
                format!(
                    "{} {}",
                    text.desktop_settings.new_version_available, version
                ),
            );
            egui::CollapsingHeader::new(text.desktop_settings.release_notes_label)
                .icon(crate::gui::widgets::collapsing_chevron)
                .show(ui, |ui| {
                    ui.label(body);
                });
            ui.add_space(5.0);
            if ui
                .button(egui::RichText::new(text.desktop_settings.download_update_btn).strong())
                .clicked()
                && let Some(u) = updater
            {
                u.perform_update();
            }
        }
        UpdateStatus::Downloading => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(text.desktop_settings.downloading_update);
            });
        }
        UpdateStatus::Error(e) => {
            ui.colored_label(
                theme.danger_text(),
                format!("{} {}", text.desktop_settings.update_failed, e),
            );
            ui.label(
                egui::RichText::new(text.desktop_settings.app_folder_writable_hint).size(11.0),
            );
            if ui.button(text.desktop_settings.retry_btn).clicked()
                && let Some(u) = updater
            {
                u.check_for_updates();
            }
        }
        UpdateStatus::UpdatedAndRestartRequired(staged) => {
            ui.label(
                egui::RichText::new(text.desktop_settings.update_success)
                    .color(theme.success())
                    .heading(),
            );
            ui.label(text.desktop_settings.restart_to_use_new_version);
            if ui.button(text.desktop_settings.restart_app_btn).clicked() {
                crate::gui::app::request_update_restart(staged.clone());
            }
        }
    }
}
