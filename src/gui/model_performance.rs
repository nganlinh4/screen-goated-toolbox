use crate::model_config::ModelConfig;
use eframe::egui;

const INTELLIGENCE_COLUMN_WIDTH: f32 = 15.0;
const INTER_COLUMN_GAP: f32 = 2.0;
const LATENCY_COLUMN_WIDTH: f32 = 32.0;
pub(crate) const PREFIX_WIDTH: f32 =
    INTELLIGENCE_COLUMN_WIDTH + INTER_COLUMN_GAP + LATENCY_COLUMN_WIDTH;
/// Matches the design system's control height so the prefix fills its row.
///
/// `allocate_ui_with_layout` places its box at the cursor, i.e. top-aligned, so
/// a box shorter than the row lifts everything inside it by half the
/// difference. Sizing it to the row height keeps the latency figure on the
/// same baseline as the rest of the row.
const PREFIX_HEIGHT: f32 = crate::gui::theme::CONTROL_HEIGHT;

pub fn format_latency_ms(milliseconds: Option<u32>) -> String {
    let Some(milliseconds) = milliseconds else {
        return "—".to_string();
    };
    let tenths = (milliseconds.saturating_add(50)) / 100;
    if tenths.is_multiple_of(10) {
        format!("{}s", tenths / 10)
    } else {
        format!("{}.{:01}s", tenths / 10, tenths % 10)
    }
}

pub fn render_prefix(ui: &mut egui::Ui, model: &ModelConfig) {
    let source = model
        .performance_source
        .as_deref()
        .unwrap_or("Performance not measured");
    render_prefix_values(
        ui,
        model.intelligence_tier,
        model.typical_latency_ms,
        source,
    );
}

pub fn render_unknown_prefix(ui: &mut egui::Ui) {
    render_prefix_values(ui, None, None, "Performance not measured");
}

fn render_prefix_values(
    ui: &mut egui::Ui,
    intelligence_tier: Option<u8>,
    typical_latency_ms: Option<u32>,
    source: &str,
) {
    ui.allocate_ui_with_layout(
        egui::vec2(PREFIX_WIDTH, PREFIX_HEIGHT),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = INTER_COLUMN_GAP;
            ui.allocate_ui_with_layout(
                egui::vec2(INTELLIGENCE_COLUMN_WIDTH, PREFIX_HEIGHT),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| match intelligence_tier {
                    Some(tier) => {
                        crate::gui::icons::draw_icon_static(
                            ui,
                            intelligence_icon(tier),
                            Some(crate::gui::icons::ICON_XS),
                        );
                    }
                    None => {
                        ui.label(egui::RichText::new("—").weak());
                    }
                },
            );
            ui.allocate_ui_with_layout(
                egui::vec2(LATENCY_COLUMN_WIDTH, PREFIX_HEIGHT),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    ui.label(
                        egui::RichText::new(format_latency_ms(typical_latency_ms))
                            .monospace()
                            .size(11.0),
                    );
                },
            );
        },
    )
    .response
    .on_hover_text(source);
}

fn intelligence_icon(tier: u8) -> crate::gui::icons::Icon {
    use crate::gui::icons::Icon;
    match tier.clamp(1, 6) {
        6 => Icon::Stat3,
        5 => Icon::Stat2,
        4 => Icon::Stat1,
        3 => Icon::StatMinus1,
        2 => Icon::StatMinus2,
        _ => Icon::StatMinus3,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        INTELLIGENCE_COLUMN_WIDTH, INTER_COLUMN_GAP, LATENCY_COLUMN_WIDTH, format_latency_ms,
        intelligence_icon,
    };
    use crate::gui::icons::Icon;

    #[test]
    fn latency_format_matches_shared_parity_fixture() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/parity-fixtures/model-catalog/presentation.json"
        )))
        .expect("model catalog presentation fixture parses");
        for case in fixture["performance"]["latency_format_cases"]
            .as_array()
            .expect("latency_format_cases must be an array")
        {
            let milliseconds = u32::try_from(case["milliseconds"].as_u64().unwrap()).unwrap();
            assert_eq!(
                format_latency_ms(Some(milliseconds)),
                case["label"].as_str().unwrap()
            );
        }
        assert_eq!(format_latency_ms(None), "—");
    }

    #[test]
    fn six_intelligence_levels_map_to_the_shared_stat_scale() {
        assert_eq!(
            (1..=6).map(intelligence_icon).collect::<Vec<_>>(),
            [
                Icon::StatMinus3,
                Icon::StatMinus2,
                Icon::StatMinus1,
                Icon::Stat1,
                Icon::Stat2,
                Icon::Stat3,
            ]
        );
    }

    #[test]
    fn compact_prefix_columns_match_shared_parity_fixture() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/parity-fixtures/model-catalog/presentation.json"
        )))
        .expect("model catalog presentation fixture parses");
        let columns = &fixture["performance_columns"];
        assert_eq!(
            INTELLIGENCE_COLUMN_WIDTH,
            columns["intelligence_width"].as_f64().unwrap() as f32
        );
        assert_eq!(
            INTER_COLUMN_GAP,
            columns["inter_column_gap"].as_f64().unwrap() as f32
        );
        assert_eq!(
            LATENCY_COLUMN_WIDTH,
            columns["latency_width"].as_f64().unwrap() as f32
        );
    }
}
