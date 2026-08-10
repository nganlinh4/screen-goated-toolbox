use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use crate::gui::locale::LocaleText;
use crate::gui::theme::AppTheme;
use crate::overlay::realtime_webview::state::REALTIME_STATE;
use eframe::egui;

use super::utils::{
    cached_probe, cached_u64, format_size, invalidate_probe_cache, invalidate_u64_cache, tool_card,
};

const PROBE_COMPUTER_CONTROL: &str = "downloaded-tools:computer-control";
const VALUE_COMPUTER_CONTROL_SIZE: &str = "downloaded-tools:computer-control-size";

pub(super) fn render_computer_control_card(ui: &mut egui::Ui, text: &LocaleText) {
    let managed = &text.auxiliary.managed_tools;
    let download_title = crate::component_registry::computer_control::download_title();
    tool_card(ui, |ui| {
        let theme = AppTheme::from_ui(ui);
        ui.heading(managed.tool_computer_control_card);
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(managed.tool_computer_control_payload).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let downloading = REALTIME_STATE
                    .lock()
                    .map(|state| state.is_downloading && state.download_title == download_title)
                    .unwrap_or(false);
                if downloading {
                    let progress = REALTIME_STATE
                        .lock()
                        .map(|state| state.download_progress)
                        .unwrap_or(0.0);
                    ui.label(format!("{progress:.0}%"));
                    ui.spinner();
                } else if cached_probe(
                    PROBE_COMPUTER_CONTROL,
                    crate::component_registry::computer_control::is_installed,
                ) {
                    if ui
                        .button(
                            egui::RichText::new(text.auxiliary.managed_tools.tool_action_delete)
                                .color(theme.danger_text()),
                        )
                        .clicked()
                    {
                        let _ = crate::component_registry::computer_control::remove();
                        invalidate_probe_cache(PROBE_COMPUTER_CONTROL);
                        invalidate_u64_cache(VALUE_COMPUTER_CONTROL_SIZE);
                    }
                    ui.label(
                        egui::RichText::new(format_size(cached_u64(
                            VALUE_COMPUTER_CONTROL_SIZE,
                            crate::component_registry::computer_control::installed_size,
                        )))
                        .color(theme.success()),
                    );
                } else {
                    if ui
                        .button(text.auxiliary.managed_tools.tool_action_download)
                        .clicked()
                    {
                        std::thread::spawn(|| {
                            let _ =
                                crate::component_registry::computer_control::download_from_manager(
                                    Arc::new(AtomicBool::new(false)),
                                    true,
                                );
                            invalidate_probe_cache(PROBE_COMPUTER_CONTROL);
                            invalidate_u64_cache(VALUE_COMPUTER_CONTROL_SIZE);
                        });
                    }
                    ui.label(
                        egui::RichText::new(text.auxiliary.managed_tools.tool_status_missing)
                            .color(egui::Color32::GRAY),
                    );
                }
            });
        });
        ui.label(managed.tool_desc_computer_control);
    });
}
