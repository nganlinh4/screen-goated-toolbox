use crate::model_config::ModelConfig;
use eframe::egui;

const QUALITY_COLUMN_WIDTH: f32 = 76.0;
const LATENCY_COLUMN_WIDTH: f32 = 42.0;
const PREFIX_HEIGHT: f32 = 18.0;

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
    ui.allocate_ui_with_layout(
        egui::vec2(QUALITY_COLUMN_WIDTH, PREFIX_HEIGHT),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| match model.quality_tier {
            Some(tier) => {
                ui.spacing_mut().item_spacing.x = 1.0;
                for _ in 0..tier {
                    crate::gui::icons::draw_icon_static(
                        ui,
                        crate::gui::icons::Icon::Psychology,
                        Some(crate::gui::icons::ICON_XS),
                    );
                }
            }
            None => {
                ui.label(egui::RichText::new("—").weak());
            }
        },
    )
    .response
    .on_hover_text(source);

    ui.allocate_ui_with_layout(
        egui::vec2(LATENCY_COLUMN_WIDTH, PREFIX_HEIGHT),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.label(
                egui::RichText::new(format_latency_ms(model.typical_latency_ms))
                    .monospace()
                    .size(11.0),
            );
        },
    )
    .response
    .on_hover_text(source);
}

#[cfg(test)]
mod tests {
    use super::format_latency_ms;

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
}
