use crate::gui::locale::LocaleText;
use crate::runtime_support::{self, WebView2InstallStatus};
use eframe::egui;

pub(super) fn render_webview2_section(ui: &mut egui::Ui, text: &LocaleText) {
    let status = runtime_support::current_webview2_status();

    ui.group(|ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(text.tool_webview2).strong());
            ui.with_layout(
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| match &status {
                    WebView2InstallStatus::Installed => {
                        ui.label(
                            egui::RichText::new(text.tool_status_installed.replace("{}", "OK"))
                                .color(egui::Color32::from_rgb(34, 139, 34)),
                        );
                    }
                    WebView2InstallStatus::Installing { progress } => {
                        if let Some(progress) = progress {
                            ui.label(format!("{progress:.0}%"));
                        }
                        ui.spinner();
                    }
                    WebView2InstallStatus::Missing => {
                        if ui.button(text.tool_action_download).clicked() {
                            let _ = runtime_support::start_webview2_runtime_install();
                        }
                        ui.label(
                            egui::RichText::new(text.tool_status_missing)
                                .color(egui::Color32::GRAY),
                        );
                    }
                    WebView2InstallStatus::Error(message) => {
                        if ui.button(text.tool_action_download).clicked() {
                            let _ = runtime_support::start_webview2_runtime_install();
                        }
                        ui.label(
                            egui::RichText::new(text.tool_status_install_failed)
                                .color(egui::Color32::RED),
                        )
                        .on_hover_text(message);
                    }
                },
            );
        });

        ui.horizontal(|ui| {
            ui.label(text.tool_desc_webview2);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(runtime_support::architecture_summary())
                        .color(egui::Color32::GRAY),
                );
            });
        });
    });
}
