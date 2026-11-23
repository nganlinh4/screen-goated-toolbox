use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use crate::gui::locale::LocaleText;
use eframe::egui;

use super::component_row::{ComponentState, RowAction, RowOperations, render_component_row};
use super::utils::{
    cached_probe, cached_u64, format_size, invalidate_probe_cache, invalidate_u64_cache,
    start_install, start_removal, tool_card,
};

const PROBE_RECORDER: &str = "downloaded-tools:screen-recorder";
const VALUE_RECORDER_SIZE: &str = "downloaded-tools:screen-recorder-size";

pub(super) fn render_recorder_card(ui: &mut egui::Ui, text: &LocaleText) {
    use crate::component_registry::recorder;

    let managed = &text.auxiliary.managed_tools;
    tool_card(ui, |ui| {
        ui.heading(managed.tool_screen_recorder_card);
        ui.add_space(4.0);
        let state = if cached_probe(PROBE_RECORDER, recorder::is_installed) {
            ComponentState::Installed {
                summary: format_size(cached_u64(VALUE_RECORDER_SIZE, recorder::installed_size)),
            }
        } else if !recorder::delivery_available() {
            ComponentState::Unavailable
        } else {
            ComponentState::Missing
        };
        let operations = RowOperations {
            install: Some(recorder::BUNDLE_ID),
            removal: PROBE_RECORDER,
        };
        match render_component_row(
            ui,
            text,
            managed.tool_screen_recorder_payload,
            operations,
            state,
        ) {
            Some(RowAction::Install) => start_install(recorder::BUNDLE_ID, || {
                let _ = recorder::download_from_manager(Arc::new(AtomicBool::new(false)), true);
                invalidate_caches();
            }),
            Some(RowAction::Remove) => start_removal(
                PROBE_RECORDER,
                managed.tool_screen_recorder_card.to_string(),
                recorder::remove_all,
                invalidate_caches,
            ),
            None => {}
        }
        ui.label(managed.tool_desc_screen_recorder);
    });
}

fn invalidate_caches() {
    invalidate_probe_cache(PROBE_RECORDER);
    invalidate_u64_cache(VALUE_RECORDER_SIZE);
}

#[cfg(test)]
mod tests {
    /// Removal start/outcome notifications have one owner, the shared
    /// Downloaded Tools removal worker; the registry only removes.
    #[test]
    fn recorder_removal_is_announced_only_by_the_shared_removal_worker() {
        let registry = include_str!("../../../../component_registry/recorder.rs");
        for template in [
            "removing_component_fmt",
            "component_removed_fmt",
            "component_remove_failed_fmt",
        ] {
            assert!(
                !registry.contains(template),
                "component_registry/recorder.rs still announces removal via {template}"
            );
        }
        let row = include_str!("recorder.rs");
        let render = &row[..row.find("#[cfg(test)]").unwrap()];
        assert!(render.contains("start_removal("));
        assert!(render.contains("recorder::remove_all"));
    }
}
