//! The models hub merges and sorts the catalog once per frame and every
//! provider card reads that one result.

use super::*;
use crate::model_config::CATALOG_BUILDS;

#[test]
fn one_frame_builds_the_merged_catalog_once_for_every_card() {
    let mut config = Config::default();
    let locale = LocaleText::get("en");
    let builds_before = CATALOG_BUILDS.with(std::cell::Cell::get);
    let mut passes = 0;
    crate::gui::test_support::run_ui(
        &egui::Context::default(),
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1400.0, 6000.0),
            )),
            ..Default::default()
        },
        |ui| {
            passes += 1;
            render_custom_models_body(ui, &mut config, &locale, 6000.0);
        },
    );
    assert!(passes > 0);
    assert_eq!(
        CATALOG_BUILDS.with(std::cell::Cell::get) - builds_before,
        passes
    );
}
