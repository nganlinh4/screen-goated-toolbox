//! Downloaded Tools card for the complete Creation product.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::gui::locale::LocaleText;
use crate::gui::theme::AppTheme;
use eframe::egui;

use super::utils::{
    cached_probe, cached_u64, format_size, get_dir_size, invalidate_probe_cache,
    invalidate_u64_cache, removal_in_progress, start_removal, tool_card,
};

const PROBE_CREATION: &str = "downloaded-tools:creation";
const PROBE_CREATION_ANY: &str = "downloaded-tools:creation-any";
const VALUE_CREATION_SIZE: &str = "downloaded-tools:creation-size";
static DOWNLOADING: AtomicBool = AtomicBool::new(false);

pub(super) fn render_three_d_generator_card(ui: &mut egui::Ui, text: &LocaleText) {
    let managed = &text.auxiliary.managed_tools;
    tool_card(ui, |ui| {
        let theme = AppTheme::from_ui(ui);
        ui.heading(managed.tool_creation_card);
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(managed.tool_creation_payload).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if removal_in_progress(PROBE_CREATION) {
                    ui.label(managed.tool_status_removing);
                    ui.spinner();
                } else if DOWNLOADING.load(Ordering::Acquire) {
                    ui.label(managed.tool_status_installing);
                    ui.spinner();
                } else if cached_probe(
                    PROBE_CREATION,
                    crate::overlay::three_d_generator::is_product_installed,
                ) {
                    if ui
                        .button(
                            egui::RichText::new(managed.tool_action_delete)
                                .color(theme.danger_text()),
                        )
                        .clicked()
                    {
                        start_product_removal(managed.tool_creation_card);
                    }
                    ui.label(
                        egui::RichText::new(format_size(cached_u64(
                            VALUE_CREATION_SIZE,
                            product_installed_size,
                        )))
                        .color(theme.success()),
                    );
                } else if !crate::overlay::three_d_generator::is_product_available() {
                    if cached_probe(
                        PROBE_CREATION_ANY,
                        crate::overlay::three_d_generator::is_product_partially_installed,
                    ) && ui
                        .button(
                            egui::RichText::new(managed.tool_action_delete)
                                .color(theme.danger_text()),
                        )
                        .clicked()
                    {
                        start_product_removal(managed.tool_creation_card);
                    }
                    ui.label(
                        egui::RichText::new(managed.tool_status_unavailable)
                            .color(theme.danger_text()),
                    );
                } else {
                    if cached_probe(
                        PROBE_CREATION_ANY,
                        crate::overlay::three_d_generator::is_product_partially_installed,
                    ) && ui
                        .button(
                            egui::RichText::new(managed.tool_action_delete)
                                .color(theme.danger_text()),
                        )
                        .clicked()
                    {
                        start_product_removal(managed.tool_creation_card);
                    }
                    if ui.button(managed.tool_action_download).clicked()
                        && DOWNLOADING
                            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                            .is_ok()
                    {
                        std::thread::spawn(|| {
                            let _ = crate::overlay::three_d_generator::download_product(
                                Arc::new(AtomicBool::new(false)),
                                true,
                            );
                            invalidate_probe_cache(PROBE_CREATION);
                            invalidate_probe_cache(PROBE_CREATION_ANY);
                            invalidate_u64_cache(VALUE_CREATION_SIZE);
                            DOWNLOADING.store(false, Ordering::Release);
                        });
                    }
                    ui.label(
                        egui::RichText::new(managed.tool_status_missing).color(egui::Color32::GRAY),
                    );
                }
            });
        });
        ui.label(managed.tool_desc_creation_product);
    });
}

fn start_product_removal(title: &'static str) {
    start_removal(
        PROBE_CREATION,
        title.to_string(),
        crate::overlay::three_d_generator::remove_product,
        || {
            invalidate_probe_cache(PROBE_CREATION);
            invalidate_probe_cache(PROBE_CREATION_ANY);
            invalidate_u64_cache(VALUE_CREATION_SIZE);
        },
    );
}

fn product_installed_size() -> u64 {
    [
        crate::overlay::three_d_generator::web_assets_dir(),
        crate::overlay::three_d_generator::runtime_bundle_dir(),
    ]
    .into_iter()
    .map(|root| get_dir_size(&root))
    .sum()
}
