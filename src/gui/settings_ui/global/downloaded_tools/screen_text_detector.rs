use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use crate::component_registry::screen_text_detector as detector;
use crate::gui::locale::LocaleText;
use eframe::egui;

use super::component_row::{ComponentState, RowAction, RowOperations, render_component_row};
use super::utils::{
    cached_probe, cached_u64, format_size, invalidate_probe_cache, invalidate_u64_cache,
    start_install, start_removal, tool_card,
};

const PROBE: &str = "downloaded-tools:screen-text-detector";
const SIZE: &str = "downloaded-tools:screen-text-detector-size";
const INSTALL: &str = "downloaded-tools:install-screen-text-detector";

pub(super) fn render(ui: &mut egui::Ui, text: &LocaleText) {
    let managed = &text.auxiliary.managed_tools;
    tool_card(ui, |ui| {
        ui.heading(managed.tool_screen_translate_card);
        ui.add_space(4.0);
        let state = if cached_probe(PROBE, detector::is_installed) {
            ComponentState::Installed {
                summary: format_size(cached_u64(SIZE, detector::installed_size)),
            }
        } else if !detector::delivery_available() {
            ComponentState::Unavailable
        } else {
            ComponentState::Missing
        };
        let operations = RowOperations {
            install: Some(INSTALL),
            removal: PROBE,
        };
        match render_component_row(
            ui,
            text,
            managed.tool_screen_translate_detector,
            operations,
            state,
        ) {
            Some(RowAction::Install) => start_install(INSTALL, install_with_dependencies),
            Some(RowAction::Remove) => start_removal(
                PROBE,
                managed.tool_screen_translate_card.to_string(),
                detector::remove,
                invalidate,
            ),
            None => {}
        }
        ui.label(managed.tool_desc_screen_translate_detector);
    });
}

/// Installs the VC runtime, the ONNX runtime and the detector under one badge.
fn install_with_dependencies() {
    let cancelled = Arc::new(AtomicBool::new(false));
    let Ok(_activity) = crate::install_activity::register(cancelled.clone()) else {
        return;
    };
    let badge =
        crate::overlay::auto_copy_badge::DownloadProgressBadge::new(&detector::localized_name());
    let result = (|| -> anyhow::Result<()> {
        let vc = crate::component_registry::vc_runtime::ensure_component(|done, total| {
            badge.report(done.saturating_mul(10), total.saturating_mul(100));
        })?;
        let runtime =
            crate::component_registry::local_asr::ensure_runtime(&cancelled, |done, total| {
                badge.report(
                    total
                        .saturating_mul(10)
                        .saturating_add(done.saturating_mul(50)),
                    total.saturating_mul(100),
                );
            })?;
        let installed = detector::ensure(&cancelled, |done, total| {
            badge.report(
                total
                    .saturating_mul(60)
                    .saturating_add(done.saturating_mul(40)),
                total.saturating_mul(100),
            );
        })?;
        drop((installed, runtime, vc));
        Ok(())
    })();
    badge.finish();
    if let Err(error) = result {
        crate::log_info!("[Screen Translate] detector install failed: {error:#}");
        show_install_failed(&error);
    }
    invalidate();
}

/// The card returns to "Missing" after a failure, so the reason has to be
/// shown where the user will see it.
fn show_install_failed(error: &anyhow::Error) {
    use crate::overlay::auto_copy_badge::{
        NotificationType, format_locale, locale_text, show_detailed_notification,
    };
    let name = detector::localized_name();
    let title = format_locale(
        locale_text().component_install_failed_fmt,
        &[("name", &name)],
    );
    show_detailed_notification(&title, &format!("{error:#}"), NotificationType::Error);
}

fn invalidate() {
    invalidate_probe_cache(PROBE);
    invalidate_u64_cache(SIZE);
}
