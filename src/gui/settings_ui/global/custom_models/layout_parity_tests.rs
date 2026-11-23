//! Replays `parity-fixtures/preset-system/custom-models-dialog.json` against the
//! rendered Windows Custom Models body (the canonical provider-card layout).

use super::*;

const USER_MODEL_NAME: &str = "Parity fixture user model";
const DISCOVERED_MODEL_NAME: &str = "Parity fixture discovered model";

fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "../../../../../parity-fixtures/preset-system/custom-models-dialog.json"
    ))
    .unwrap()
}

fn rendered_text(config: &mut Config) -> Vec<(String, egui::Pos2)> {
    fn collect(shape: &egui::epaint::Shape, text: &mut Vec<(String, egui::Pos2)>) {
        match shape {
            egui::epaint::Shape::Text(value) => {
                text.push((value.galley.text().to_string(), value.pos))
            }
            egui::epaint::Shape::Vec(shapes) => {
                shapes.iter().for_each(|shape| collect(shape, text))
            }
            _ => {}
        }
    }
    let locale = LocaleText::get("en");
    let output = crate::gui::test_support::run_ui(
        &egui::Context::default(),
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1400.0, 6000.0),
            )),
            ..Default::default()
        },
        |ui| {
            assert!(!render_custom_models_body(ui, config, &locale, 6000.0));
        },
    );
    let mut text = Vec::new();
    for shape in output.shapes {
        collect(&shape.shape, &mut text);
    }
    text
}

fn provider_id(fixture_provider: &str) -> &'static str {
    match fixture_provider {
        "GOOGLE" => "google",
        "GROQ" => "groq",
        "OPENROUTER" => "openrouter",
        "OLLAMA" => "ollama",
        other => panic!("unknown fixture provider {other}"),
    }
}

/// A card title is the leftmost rendering of the provider label; the add CTA
/// that repeats the label is pinned to the card's right edge.
fn card_title(rendered: &[(String, egui::Pos2)], provider: &str) -> egui::Pos2 {
    rendered
        .iter()
        .filter(|(text, _)| text == provider_label(provider))
        .map(|(_, position)| *position)
        .min_by(|a, b| a.x.total_cmp(&b.x))
        .unwrap_or_else(|| panic!("missing card title for {provider}"))
}

/// The card that contains `position`: same column, nearest title above it.
fn owning_card<'a>(titles: &'a [(&'static str, egui::Pos2)], position: egui::Pos2) -> &'a str {
    let column = |x: f32| x >= 700.0;
    titles
        .iter()
        .filter(|(_, title)| column(title.x) == column(position.x) && title.y <= position.y + 1.0)
        .max_by(|a, b| a.1.y.total_cmp(&b.1.y))
        .map(|(provider, _)| *provider)
        .unwrap_or_else(|| panic!("no card owns {position:?}"))
}

#[test]
fn shared_custom_models_cards_follow_the_canonical_order_and_own_their_actions() {
    let fixture = fixture();
    let mut config = Config::default();
    config.custom_models.push(CustomModelDefinition {
        id: "custom-openrouter-parity-fixture".into(),
        provider: "openrouter".into(),
        display_name: USER_MODEL_NAME.into(),
        full_name: "vendor/parity-fixture".into(),
        model_type: CustomModelType::Text,
        enabled: true,
        quota_en: String::new(),
        quota_vi: String::new(),
        quota_ko: String::new(),
        supports_search: None,
    });
    let _discovered = crate::model_config::DiscoveredForTest::text_model(
        "ollama-parity-fixture-discovered",
        DISCOVERED_MODEL_NAME,
    );
    let before = serde_json::to_value(&config).unwrap();
    let rendered = rendered_text(&mut config);
    assert_eq!(serde_json::to_value(&config).unwrap(), before);

    let expected: Vec<&'static str> = fixture["editable_provider_order"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| provider_id(value.as_str().unwrap()))
        .collect();
    let mut titles: Vec<_> = expected
        .iter()
        .map(|provider| (*provider, card_title(&rendered, provider)))
        .collect();
    // Reading order of the two-column card grid: left column first, then
    // each column top to bottom.
    titles.sort_by(|a, b| {
        (a.1.x >= 700.0)
            .cmp(&(b.1.x >= 700.0))
            .then(a.1.y.total_cmp(&b.1.y))
    });
    let order: Vec<_> = titles.iter().map(|(provider, _)| *provider).collect();
    assert_eq!(order, expected);

    let text = LocaleText::get("en");
    for action in fixture["provider_card_actions"].as_array().unwrap() {
        let (label, provider) = match action.as_str().unwrap() {
            "add_openrouter" => (add_label("openrouter", &text), "openrouter"),
            "scan_openrouter" => (
                text.model_catalog
                    .custom_models_import_openrouter
                    .to_string(),
                "openrouter",
            ),
            "scan_ollama" => (
                text.model_catalog.custom_models_scan_ollama.to_string(),
                "ollama",
            ),
            other => panic!("unknown fixture action {other}"),
        };
        let positions: Vec<_> = rendered
            .iter()
            .filter(|(value, _)| *value == label)
            .map(|(_, position)| *position)
            .collect();
        assert_eq!(positions.len(), 1, "{label} must render exactly once");
        let title = titles.iter().find(|(id, _)| *id == provider).unwrap().1;
        assert_eq!(owning_card(&titles, positions[0]), provider, "{label}");
        assert!(
            (positions[0].y - title.y).abs() < 16.0,
            "{label} belongs to the {provider} card header row"
        );
    }

    for section in fixture["required_sections"].as_array().unwrap() {
        match section.as_str().unwrap() {
            "built_in_locked" => {
                let caption = text.model_catalog.custom_models_builtin_locked;
                let owners: Vec<_> = rendered
                    .iter()
                    .filter(|(value, _)| value == caption)
                    .map(|(_, position)| owning_card(&titles, *position))
                    .collect();
                assert!(owners.contains(&"google"), "{owners:?}");
            }
            "user_models" => {
                let row = rendered
                    .iter()
                    .find(|(value, _)| value == USER_MODEL_NAME)
                    .expect("user model row is rendered as an editable field");
                assert_eq!(owning_card(&titles, row.1), "openrouter");
            }
            "discovered" => {
                let caption = text.model_catalog.custom_models_discovered_models;
                let caption = rendered
                    .iter()
                    .find(|(value, _)| value == caption)
                    .expect("discovered models have their own section");
                let row = rendered
                    .iter()
                    .find(|(value, _)| value == DISCOVERED_MODEL_NAME)
                    .expect("a discovered model is listed as a locked row");
                assert_eq!(owning_card(&titles, caption.1), "ollama");
                assert_eq!(owning_card(&titles, row.1), "ollama");
                assert!(row.1.y > caption.1.y, "the row sits under its caption");
            }
            other => panic!("unknown fixture section {other}"),
        }
    }
}
