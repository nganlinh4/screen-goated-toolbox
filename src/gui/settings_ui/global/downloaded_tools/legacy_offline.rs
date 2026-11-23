//! The "Legacy offline models" row. It appears only while folders from removed
//! offline speech engines remain on disk, and removes them only on click.

use crate::component_registry::legacy_offline;
use crate::gui::locale::LocaleText;
use eframe::egui;

use super::component_row::{ComponentState, RowAction, RowOperations, render_component_row};
use super::utils::{
    cached_probe, cached_u64, format_size, invalidate_probe_cache, invalidate_u64_cache,
    removal_in_progress, start_removal, tool_card,
};

const PROBE_LEGACY_OFFLINE: &str = "downloaded-tools:legacy-offline";
const VALUE_LEGACY_OFFLINE_SIZE: &str = "downloaded-tools:legacy-offline-size";

pub(super) fn render(ui: &mut egui::Ui, text: &LocaleText) {
    let present = cached_probe(PROBE_LEGACY_OFFLINE, || {
        legacy_offline::scan().is_ok_and(|found| !found.paths.is_empty())
    });
    if !present && !removal_in_progress(PROBE_LEGACY_OFFLINE) {
        return;
    }
    let managed = &text.auxiliary.managed_tools;
    ui.add_space(8.0);
    tool_card(ui, |ui| {
        ui.heading(managed.tool_legacy_offline_card);
        ui.add_space(4.0);
        let size = cached_u64(VALUE_LEGACY_OFFLINE_SIZE, || {
            legacy_offline::scan()
                .map(|found| found.total_bytes)
                .unwrap_or(0)
        });
        let state = ComponentState::Installed {
            summary: format_size(size),
        };
        let operations = RowOperations {
            install: None,
            removal: PROBE_LEGACY_OFFLINE,
        };
        if render_component_row(
            ui,
            text,
            managed.tool_legacy_offline_files,
            operations,
            state,
        ) == Some(RowAction::Remove)
        {
            start_removal(
                PROBE_LEGACY_OFFLINE,
                managed.tool_legacy_offline_card.to_string(),
                legacy_offline::remove,
                || {
                    invalidate_probe_cache(PROBE_LEGACY_OFFLINE);
                    invalidate_u64_cache(VALUE_LEGACY_OFFLINE_SIZE);
                },
            );
        }
        ui.label(managed.tool_desc_legacy_offline);
    });
}
