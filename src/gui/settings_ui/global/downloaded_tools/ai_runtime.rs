use crate::gui::locale::LocaleText;
use crate::unpack_dlls::{self, AiRuntimeStatus};
use eframe::egui;

use super::utils::format_size;

pub(super) fn render_ai_runtime_section(ui: &mut egui::Ui, text: &LocaleText) {
    let status = unpack_dlls::current_ai_runtime_status();

    ui.group(|ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(text.tool_ai_runtime).strong());
            ui.with_layout(
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| match &status {
                    AiRuntimeStatus::Installed { bytes } => {
                        if ui
                            .button(
                                egui::RichText::new(text.tool_action_delete)
                                    .color(egui::Color32::RED),
                            )
                            .clicked()
                        {
                            let _ = unpack_dlls::remove_ai_runtime();
                        }

                        ui.label(
                            egui::RichText::new(
                                text.tool_status_installed
                                    .replace("{}", &format_size(*bytes)),
                            )
                            .color(egui::Color32::from_rgb(34, 139, 34)),
                        );
                    }
                    AiRuntimeStatus::Installing { label, progress } => {
                        ui.label(format!("{:.0}%", progress));
                        ui.spinner();
                        ui.label(label);
                    }
                    AiRuntimeStatus::Error(message) => {
                        if ui.button(text.tool_action_download).clicked() {
                            let _ = unpack_dlls::start_ai_runtime_install();
                        }
                        ui.label(
                            egui::RichText::new(text.tool_status_install_failed)
                                .color(egui::Color32::RED),
                        )
                        .on_hover_text(message);
                    }
                    AiRuntimeStatus::Missing => {
                        if ui.button(text.tool_action_download).clicked() {
                            let _ = unpack_dlls::start_ai_runtime_install();
                        }
                        ui.label(
                            egui::RichText::new(text.tool_status_missing)
                                .color(egui::Color32::GRAY),
                        );
                    }
                },
            );
        });

        ui.label(text.tool_desc_ai_runtime);
    });
}
