use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use crate::gui::locale::LocaleText;
use eframe::egui;

use super::component_row::{ComponentState, RowAction, RowOperations, render_component_row};
use super::utils::{
    cached_probe, cached_u64, format_size, invalidate_probe_cache, invalidate_u64_cache,
    start_install, start_removal, tool_card,
};

const PROBE_COMPUTER_CONTROL: &str = "downloaded-tools:computer-control";
const VALUE_COMPUTER_CONTROL_SIZE: &str = "downloaded-tools:computer-control-size";

pub(super) fn render_computer_control_card(ui: &mut egui::Ui, text: &LocaleText) {
    use crate::component_registry::computer_control;

    let managed = &text.auxiliary.managed_tools;
    tool_card(ui, |ui| {
        ui.heading(managed.tool_computer_control_card);
        ui.add_space(4.0);
        let state = if cached_probe(PROBE_COMPUTER_CONTROL, computer_control::is_installed) {
            ComponentState::Installed {
                summary: format_size(cached_u64(
                    VALUE_COMPUTER_CONTROL_SIZE,
                    computer_control::installed_size,
                )),
            }
        } else if !computer_control::delivery_available() {
            ComponentState::Unavailable
        } else {
            ComponentState::Missing
        };
        let operations = RowOperations {
            install: Some(computer_control::ID),
            removal: PROBE_COMPUTER_CONTROL,
        };
        match render_component_row(
            ui,
            text,
            managed.tool_computer_control_payload,
            operations,
            state,
        ) {
            Some(RowAction::Install) => start_install(computer_control::ID, || {
                let _ =
                    computer_control::download_from_manager(Arc::new(AtomicBool::new(false)), true);
                invalidate_probe_cache(PROBE_COMPUTER_CONTROL);
                invalidate_u64_cache(VALUE_COMPUTER_CONTROL_SIZE);
            }),
            Some(RowAction::Remove) => start_removal(
                PROBE_COMPUTER_CONTROL,
                managed.tool_computer_control_card.to_string(),
                crate::overlay::computer_control::remove_downloaded_engine,
                || {
                    invalidate_probe_cache(PROBE_COMPUTER_CONTROL);
                    invalidate_u64_cache(VALUE_COMPUTER_CONTROL_SIZE);
                },
            ),
            None => {}
        }
        ui.label(managed.tool_desc_computer_control);
    });
}
