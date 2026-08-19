use std::sync::atomic::{AtomicBool, Ordering};

use crate::gui::locale::LocaleText;
use crate::gui::theme::AppTheme;
use eframe::egui;

use super::utils::{
    cached_probe, cached_u64, format_size, invalidate_probe_cache, invalidate_u64_cache,
    removal_in_progress, start_removal, tool_card,
};

const PROBE: &str = "downloaded-tools:screen-text-detector";
const SIZE: &str = "downloaded-tools:screen-text-detector-size";
static INSTALLING: AtomicBool = AtomicBool::new(false);

pub(super) fn render(ui: &mut egui::Ui, text: &LocaleText) {
    let managed = &text.auxiliary.managed_tools;
    tool_card(ui, |ui| {
        let theme = AppTheme::from_ui(ui);
        ui.heading(managed.tool_screen_translate_card);
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(managed.tool_screen_translate_detector).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if removal_in_progress(PROBE) {
                    ui.label(managed.tool_status_removing);
                    ui.spinner();
                } else if INSTALLING.load(Ordering::Acquire) {
                    ui.label(managed.tool_status_installing);
                    ui.spinner();
                } else if cached_probe(
                    PROBE,
                    crate::component_registry::screen_text_detector::is_installed,
                ) {
                    if ui
                        .button(
                            egui::RichText::new(managed.tool_action_delete)
                                .color(theme.danger_text()),
                        )
                        .clicked()
                    {
                        start_removal(
                            PROBE,
                            managed.tool_screen_translate_card.to_string(),
                            crate::component_registry::screen_text_detector::remove,
                            invalidate,
                        );
                    }
                    ui.label(
                        egui::RichText::new(format_size(cached_u64(
                            SIZE,
                            crate::component_registry::screen_text_detector::installed_size,
                        )))
                        .color(theme.success()),
                    );
                } else if !crate::component_registry::screen_text_detector::delivery_available() {
                    ui.label(
                        egui::RichText::new(managed.tool_status_unavailable)
                            .color(theme.danger_text()),
                    );
                } else {
                    if ui.button(managed.tool_action_download).clicked()
                        && INSTALLING
                            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                            .is_ok()
                    {
                        std::thread::spawn(|| {
                            let cancelled = AtomicBool::new(false);
                            let badge = crate::overlay::auto_copy_badge::DownloadProgressBadge::new(
                                &crate::component_registry::screen_text_detector::localized_name(),
                            );
                            let result = (|| -> anyhow::Result<()> {
                                let vc = crate::component_registry::vc_runtime::ensure_component(
                                    |done, total| {
                                        badge.report(
                                            done.saturating_mul(10),
                                            total.saturating_mul(100),
                                        );
                                    },
                                )?;
                                let runtime = crate::component_registry::local_asr::ensure_runtime(
                                    &cancelled,
                                    |done, total| {
                                        badge.report(
                                            total
                                                .saturating_mul(10)
                                                .saturating_add(done.saturating_mul(50)),
                                            total.saturating_mul(100),
                                        );
                                    },
                                )?;
                                let detector =
                                    crate::component_registry::screen_text_detector::ensure(
                                        &cancelled,
                                        |done, total| {
                                            badge.report(
                                                total
                                                    .saturating_mul(60)
                                                    .saturating_add(done.saturating_mul(40)),
                                                total.saturating_mul(100),
                                            );
                                        },
                                    )?;
                                drop((detector, runtime, vc));
                                Ok(())
                            })();
                            if let Err(error) = result {
                                crate::log_info!(
                                    "[Screen Translate] detector install failed: {error:#}"
                                );
                            }
                            badge.finish();
                            invalidate();
                            INSTALLING.store(false, Ordering::Release);
                        });
                    }
                    ui.label(
                        egui::RichText::new(managed.tool_status_missing).color(egui::Color32::GRAY),
                    );
                }
            });
        });
        ui.label(managed.tool_desc_screen_translate_detector);
    });
}

fn invalidate() {
    invalidate_probe_cache(PROBE);
    invalidate_u64_cache(SIZE);
}
