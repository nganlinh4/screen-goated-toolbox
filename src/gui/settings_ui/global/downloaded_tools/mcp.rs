//! Downloaded-Tools card for the MCP app-control capability store: lists the curated
//! integrations with install / delete per row. Data-driven (one row per catalog entry),
//! so it can't reuse `ModelRowSpec` (whose `fn` pointers can't carry an id) — but it
//! mirrors that idiom. State + actions come from `computer_control::mcp`.

use crate::gui::locale::LocaleText;
use crate::gui::theme::AppTheme;
use eframe::egui;

use super::utils::tool_card;

pub(super) fn render_mcp_card(ui: &mut egui::Ui, text: &LocaleText) {
    tool_card(ui, |ui| {
        ui.heading("App integrations (MCP)");
        ui.label(
            egui::RichText::new(
                "Curated, consent-gated integrations that drive a specific app's real API instead of clicking its UI.",
            )
            .weak(),
        );
        ui.add_space(6.0);
        for integ in crate::overlay::computer_control::ui_list() {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(integ.display_name).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let theme = AppTheme::from_ui(ui);
                    if integ.installing {
                        ui.spinner();
                        ui.label("installing…");
                    } else if integ.installed {
                        if ui
                            .button(
                                egui::RichText::new(
                                    text.auxiliary.managed_tools.tool_action_delete,
                                )
                                .color(theme.danger_text()),
                            )
                            .clicked()
                        {
                            crate::overlay::computer_control::ui_remove(integ.id);
                        }
                        let label = if integ.connected {
                            "connected"
                        } else {
                            "installed"
                        };
                        ui.label(egui::RichText::new(label).color(theme.success()));
                    } else {
                        if ui
                            .button(text.auxiliary.managed_tools.tool_action_download)
                            .clicked()
                        {
                            crate::overlay::computer_control::ui_install(integ.id);
                        }
                        ui.label(
                            egui::RichText::new(text.auxiliary.managed_tools.tool_status_missing)
                                .color(egui::Color32::GRAY),
                        );
                    }
                });
            });
            ui.label(integ.description);
            if integ.addon_hint.is_some() {
                ui.label(
                    egui::RichText::new(
                        "Needs an in-app connection — the agent sets it up on install.",
                    )
                    .color(AppTheme::from_ui(ui).warning()),
                );
            }
            ui.add_space(8.0);
        }
    });
}
