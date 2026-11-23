use super::*;

fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "../../../../../parity-fixtures/mobile-shell/credentials-provider-order.json"
    ))
    .unwrap()
}

fn frame(
    ctx: &egui::Context,
    config: &mut Config,
    events: Vec<egui::Event>,
) -> (bool, Vec<(String, egui::Pos2)>) {
    let mut changed = false;
    let mut visibility = [false; 4];
    let output = crate::gui::test_support::run_ui(
        ctx,
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1400.0, 1600.0),
            )),
            events,
            ..Default::default()
        },
        |ui| {
            let [groq, gemini, openrouter, nvidia] = &mut visibility;
            changed = render_api_keys_card(
                ui,
                config,
                ApiKeyVisibility {
                    groq,
                    gemini,
                    openrouter,
                    nvidia,
                },
                &LocaleText::get("en"),
                ApiKeyCardStyle {
                    background: egui::Color32::BLACK,
                    stroke: egui::Stroke::NONE,
                },
            );
        },
    );
    fn collect(shape: &egui::epaint::Shape, text: &mut Vec<(String, egui::Pos2)>) {
        match shape {
            egui::epaint::Shape::Text(value) => text.push((
                value.galley.text().to_string(),
                value.pos + egui::vec2(2.0, 2.0),
            )),
            egui::epaint::Shape::Vec(shapes) => {
                shapes.iter().for_each(|shape| collect(shape, text))
            }
            _ => {}
        }
    }
    let mut text = Vec::new();
    for shape in output.shapes {
        collect(&shape.shape, &mut text);
    }
    (changed, text)
}

fn enabled_config() -> Config {
    Config {
        use_groq: true,
        use_gemini: true,
        use_openrouter: true,
        use_nvidia: true,
        use_ollama: true,
        ..Default::default()
    }
}

#[test]
fn shared_credentials_order_matches_rendered_toggles_and_fields() {
    let fixture = fixture();
    let case = &fixture["cases"][0];
    assert_eq!(
        case["canonical_source"],
        "src/gui/settings_ui/global/api_keys.rs"
    );
    let text = LocaleText::get("en");
    let rows = [
        (
            "Groq",
            text.preset_basics.use_groq_checkbox,
            text.global_settings.groq_label,
        ),
        (
            "Gemini",
            text.preset_basics.use_gemini_checkbox,
            text.preset_basics.gemini_api_key_label,
        ),
        (
            "OpenRouter",
            text.preset_basics.use_openrouter_checkbox,
            text.preset_basics.openrouter_api_key_label,
        ),
        (
            "NVIDIA",
            text.preset_basics.use_nvidia_checkbox,
            text.preset_basics.nvidia_api_key_label,
        ),
        ("Ollama", "Ollama", text.global_settings.ollama_url_label),
    ];
    let mut config = enabled_config();
    let before = serde_json::to_value(&config).unwrap();
    let (changed, rendered) = frame(&egui::Context::default(), &mut config, vec![]);
    assert!(!changed);
    assert_eq!(before, serde_json::to_value(&config).unwrap());
    for column in [1, 2] {
        let actual: Vec<_> = rendered
            .iter()
            .filter_map(|(label, _)| {
                rows.iter()
                    .find(|row| label == if column == 1 { row.1 } else { row.2 })
                    .map(|row| row.0)
            })
            .collect();
        let expected: Vec<_> = case["order"]
            .as_array()
            .unwrap()
            .iter()
            .map(|name| name.as_str().unwrap())
            .collect();
        assert_eq!(actual, expected);
    }
}

#[test]
fn disabling_a_provider_only_changes_its_enable_flag() {
    let fixture = fixture();
    assert_eq!(
        fixture["cases"][0]["disable_preserves_authored_priority_rows"],
        true
    );
    let text = LocaleText::get("en");
    for (label, key) in [
        (text.preset_basics.use_groq_checkbox, "use_groq"),
        (text.preset_basics.use_gemini_checkbox, "use_gemini"),
        (text.preset_basics.use_openrouter_checkbox, "use_openrouter"),
        (text.preset_basics.use_nvidia_checkbox, "use_nvidia"),
        ("Ollama", "use_ollama"),
    ] {
        let ctx = egui::Context::default();
        let mut config = enabled_config();
        config.adaptive_model_priority.text_to_text_overrides.pinned =
            vec!["custom-provider/custom-model".to_string()];
        let (_, rendered) = frame(&ctx, &mut config, vec![]);
        let position = rendered.iter().find(|(value, _)| value == label).unwrap().1;
        let mut expected = serde_json::to_value(&config).unwrap();
        expected[key] = serde_json::Value::Bool(false);
        let events = vec![
            egui::Event::PointerMoved(position),
            egui::Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            },
        ];
        assert!(frame(&ctx, &mut config, events).0, "{label}");
        assert_eq!(serde_json::to_value(&config).unwrap(), expected, "{label}");
    }
}
