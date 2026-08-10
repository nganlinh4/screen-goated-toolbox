use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::gui::locale::LocaleText;
use crate::gui::theme::AppTheme;
use crate::overlay::realtime_webview::state::REALTIME_STATE;
use eframe::egui;

use super::utils::{
    cached_probe, cached_u64, format_size, invalidate_probe_cache, invalidate_u64_cache, tool_card,
};

const PROBE_RECORDER: &str = "downloaded-tools:screen-recorder";
const VALUE_RECORDER_SIZE: &str = "downloaded-tools:screen-recorder-size";
static REMOVING: AtomicBool = AtomicBool::new(false);

pub(super) fn render_recorder_card(ui: &mut egui::Ui, text: &LocaleText) {
    let managed = &text.auxiliary.managed_tools;
    let download_title = crate::component_registry::recorder::download_title();
    tool_card(ui, |ui| {
        let theme = AppTheme::from_ui(ui);
        ui.heading(managed.tool_screen_recorder_card);
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(managed.tool_screen_recorder_payload).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let downloading = REALTIME_STATE
                    .lock()
                    .map(|state| state.is_downloading && state.download_title == download_title)
                    .unwrap_or(false);
                if REMOVING.load(Ordering::Acquire) {
                    ui.label(managed.tool_status_removing);
                    ui.spinner();
                } else if downloading {
                    let progress = REALTIME_STATE
                        .lock()
                        .map(|state| state.download_progress)
                        .unwrap_or(0.0);
                    ui.label(format!("{progress:.0}%"));
                    ui.spinner();
                } else if cached_probe(
                    PROBE_RECORDER,
                    crate::component_registry::recorder::is_installed,
                ) {
                    if ui
                        .button(
                            egui::RichText::new(text.auxiliary.managed_tools.tool_action_delete)
                                .color(theme.danger_text()),
                        )
                        .clicked()
                        && REMOVING
                            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                            .is_ok()
                    {
                        std::thread::spawn(|| {
                            let _ = crate::component_registry::recorder::remove_from_manager();
                            invalidate_probe_cache(PROBE_RECORDER);
                            invalidate_u64_cache(VALUE_RECORDER_SIZE);
                            REMOVING.store(false, Ordering::Release);
                        });
                    }
                    ui.label(
                        egui::RichText::new(format_size(cached_u64(
                            VALUE_RECORDER_SIZE,
                            crate::component_registry::recorder::installed_size,
                        )))
                        .color(theme.success()),
                    );
                } else if !crate::component_registry::recorder::delivery_available() {
                    ui.label(
                        egui::RichText::new(managed.tool_status_unavailable)
                            .color(theme.danger_text()),
                    );
                } else {
                    if ui
                        .button(text.auxiliary.managed_tools.tool_action_download)
                        .clicked()
                    {
                        std::thread::spawn(|| {
                            let _ = crate::component_registry::recorder::download_from_manager(
                                Arc::new(AtomicBool::new(false)),
                                true,
                            );
                            invalidate_probe_cache(PROBE_RECORDER);
                            invalidate_u64_cache(VALUE_RECORDER_SIZE);
                        });
                    }
                    ui.label(
                        egui::RichText::new(text.auxiliary.managed_tools.tool_status_missing)
                            .color(egui::Color32::GRAY),
                    );
                }
            });
        });
        ui.label(managed.tool_desc_screen_recorder);
    });
}
