//! Headless geometry checks for the preset grid.
//!
//! The sidebar packs three preset lanes into one `egui::Grid`, where a row's
//! height is the tallest cell in it and each cell centres its own content. That
//! makes vertical drift between lanes easy to introduce and impossible to see in
//! a unit test on rects alone, so these tests lay the real sidebar out in a
//! headless context and read the resulting widget rects back.

use super::*;
use crate::config::Config;
use crate::gui::locale::LocaleText;

/// Lay the real sidebar out once and hand back the context holding its rects.
fn lay_out_sidebar(config: &mut Config) -> egui::Context {
    let ctx = egui::Context::default();
    let text = LocaleText::get("en");
    let mut view_mode = ViewMode::Global;
    // Two passes: an egui Grid measures its column widths on the first frame and
    // only settles on the second.
    for _ in 0..2 {
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.set_max_width(900.0);
            render_sidebar(ui, config, &mut view_mode, &text);
        });
    }
    ctx
}

/// Row centre of every preset that laid out a drag handle, per lane.
fn handle_centers(ctx: &egui::Context, config: &Config) -> Vec<(String, f32, f32)> {
    config
        .presets
        .iter()
        .filter_map(|preset| {
            let id = egui::Id::new("preset-drag-handle").with(&preset.id);
            let rect = ctx.read_response(id)?.rect;
            Some((preset.id.clone(), rect.center().x, rect.center().y))
        })
        .collect()
}

#[test]
fn every_lane_puts_its_row_icons_on_the_same_line() {
    let mut config = Config::default();
    let ctx = lay_out_sidebar(&mut config);
    let centers = handle_centers(&ctx, &config);
    assert!(!centers.is_empty(), "no preset rows laid out");

    // Group rows by lane (x), then check the i-th row of each lane shares a y.
    let mut lanes: std::collections::BTreeMap<i32, Vec<f32>> = std::collections::BTreeMap::new();
    for (_, x, y) in &centers {
        lanes.entry(x.round() as i32).or_default().push(*y);
    }
    let mut lanes: Vec<Vec<f32>> = lanes.into_values().collect();
    for lane in &mut lanes {
        lane.sort_by(f32::total_cmp);
    }
    let rows = lanes.iter().map(Vec::len).min().unwrap_or(0);
    for row in 0..rows {
        let ys: Vec<f32> = lanes.iter().map(|lane| lane[row]).collect();
        let spread = ys.iter().copied().fold(f32::MIN, f32::max)
            - ys.iter().copied().fold(f32::MAX, f32::min);
        assert!(spread < 0.6, "row {row} lanes disagree on y: {ys:?}");
    }
}

#[test]
fn rows_within_a_lane_keep_one_pitch_all_the_way_to_its_add_button() {
    let mut config = Config::default();
    let ctx = lay_out_sidebar(&mut config);
    let centers = handle_centers(&ctx, &config);

    let mut lanes: std::collections::BTreeMap<i32, Vec<f32>> = std::collections::BTreeMap::new();
    for (_, x, y) in &centers {
        lanes.entry(x.round() as i32).or_default().push(*y);
    }
    for (x, mut ys) in lanes {
        ys.sort_by(f32::total_cmp);
        let pitches: Vec<f32> = ys.windows(2).map(|w| w[1] - w[0]).collect();
        let min = pitches.iter().copied().fold(f32::MAX, f32::min);
        let max = pitches.iter().copied().fold(f32::MIN, f32::max);
        assert!(
            max - min < 0.6,
            "lane at x={x} has uneven row pitch: {pitches:?}"
        );
    }
}

#[test]
#[ignore = "diagnostic dump"]
fn dump_row_geometry() {
    let mut config = Config::default();
    let ctx = lay_out_sidebar(&mut config);
    for preset in &config.presets {
        let id = egui::Id::new("preset-drag-handle").with(&preset.id);
        if let Some(r) = ctx.read_response(id) {
            println!(
                "{:<34} x {:7.2}..{:7.2}  y {:7.2}..{:7.2}  h {:5.2}",
                preset.id,
                r.rect.left(),
                r.rect.right(),
                r.rect.top(),
                r.rect.bottom(),
                r.rect.height()
            );
        }
    }
}
