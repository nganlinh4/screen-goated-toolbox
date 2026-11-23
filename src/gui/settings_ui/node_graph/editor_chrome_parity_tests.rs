//! Replays the node-defaults and inline editor chrome parts of
//! `parity-fixtures/preset-system/node-graph-editor.json` against the rendered
//! Windows node body.

use super::{ChainNode, ChainViewer};
use crate::config::Config;
use crate::gui::locale::LocaleText;
use eframe::egui;
use egui_snarl::Snarl;

fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "../../../../parity-fixtures/preset-system/node-graph-editor.json"
    ))
    .unwrap()
}

fn rendered_node_text(lang: &str, streaming: bool, empty_prompt: bool) -> Vec<String> {
    fn collect(shape: &egui::epaint::Shape, text: &mut Vec<String>) {
        match shape {
            egui::epaint::Shape::Text(value) => text.push(value.galley.text().to_string()),
            egui::epaint::Shape::Vec(shapes) => {
                shapes.iter().for_each(|shape| collect(shape, text))
            }
            _ => {}
        }
    }
    let config = Config {
        ui_language: lang.to_string(),
        ..Default::default()
    };
    let locale = LocaleText::get(lang);
    let mut node = ChainNode::default();
    if let ChainNode::Process {
        streaming_enabled,
        prompt,
        ..
    } = &mut node
    {
        *streaming_enabled = streaming;
        if empty_prompt {
            prompt.clear();
        }
    }
    let mut snarl = Snarl::new();
    let node_id = snarl.insert_node(egui::Pos2::ZERO, node);
    let mut viewer = ChainViewer::new(&locale, &config, "text");
    let output = crate::gui::test_support::run_ui(
        &egui::Context::default(),
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 800.0),
            )),
            ..Default::default()
        },
        |ui| super::body::show_body(&mut viewer, node_id, ui, &mut snarl),
    );
    assert!(!viewer.changed);
    let mut text = Vec::new();
    for shape in output.shapes {
        collect(&shape.shape, &mut text);
    }
    text
}

#[test]
fn shared_new_text_node_uses_the_catalog_text_default() {
    let fixture = fixture();
    assert_eq!(
        ChainNode::default().to_block().model,
        fixture["default_models"]["text"].as_str().unwrap()
    );
}

#[test]
fn shared_inline_node_chrome_is_rendered_in_the_ui_language() {
    let fixture = fixture();
    let case = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == "inline_node_editor_chrome_comes_from_language")
        .unwrap();
    for locale in case["locales"].as_array().unwrap() {
        let lang = locale["lang"].as_str().unwrap();
        for streaming in [true, false] {
            let rendered = rendered_node_text(lang, streaming, false);
            let stream_key = if streaming {
                "expected_stream_on"
            } else {
                "expected_stream_off"
            };
            for key in [
                "expected_model_label",
                "expected_prompt_label",
                "expected_add_language",
                stream_key,
            ] {
                let expected = locale[key].as_str().unwrap();
                assert!(
                    rendered.iter().any(|text| text == expected),
                    "{lang} {key}: {expected:?} not in {rendered:?}"
                );
            }
        }
    }
}

#[test]
fn shared_empty_prompt_shows_the_localized_placeholder() {
    let fixture = fixture();
    let case = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == "inline_node_editor_chrome_comes_from_language")
        .unwrap();
    for locale in case["locales"].as_array().unwrap() {
        let lang = locale["lang"].as_str().unwrap();
        let expected = locale["expected_prompt_placeholder"].as_str().unwrap();
        let rendered = rendered_node_text(lang, true, true);
        assert!(
            rendered.iter().any(|text| text == expected),
            "{lang}: {expected:?} not in {rendered:?}"
        );
    }
}

#[test]
fn shared_new_special_nodes_start_on_their_input_default() {
    let fixture = fixture();
    for kind in ["image", "audio"] {
        let node = ChainNode::new_special(kind);
        assert!(node.is_special());
        let block = node.to_block();
        assert_eq!(block.block_type, kind);
        assert_eq!(
            block.model,
            fixture["default_models"][kind].as_str().unwrap(),
            "{kind} node"
        );
    }
}

fn texts(output: &egui::FullOutput) -> Vec<(String, egui::Rect)> {
    fn collect(shape: &egui::epaint::Shape, found: &mut Vec<(String, egui::Rect)>) {
        match shape {
            egui::epaint::Shape::Text(value) => found.push((
                value.galley.text().to_string(),
                value.galley.rect.translate(value.pos.to_vec2()),
            )),
            egui::epaint::Shape::Vec(shapes) => {
                shapes.iter().for_each(|shape| collect(shape, found))
            }
            _ => {}
        }
    }
    let mut found = Vec::new();
    for shape in &output.shapes {
        collect(&shape.shape, &mut found);
    }
    found
}

/// Opens the `{language1}` selector of a rendered node and returns every text
/// the open popup draws.
fn opened_language_popup_text(lang: &str) -> Vec<String> {
    let config = Config {
        ui_language: lang.to_string(),
        ..Default::default()
    };
    let locale = LocaleText::get(lang);
    let mut snarl = Snarl::new();
    let node_id = snarl.insert_node(egui::Pos2::ZERO, ChainNode::default());
    let mut viewer = ChainViewer::new(&locale, &config, "text");
    let ctx = egui::Context::default();
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 800.0));
    let mut frame = |events: Vec<egui::Event>| {
        crate::gui::test_support::run_ui(
            &ctx,
            egui::RawInput {
                screen_rect: Some(screen),
                events,
                ..Default::default()
            },
            |ui| super::body::show_body(&mut viewer, node_id, ui, &mut snarl),
        )
    };
    let first = frame(Vec::new());
    let (_, button) = texts(&first)
        .into_iter()
        .find(|(text, _)| text == "Vietnamese")
        .expect("the {language1} selector shows its current language");
    let at = button.center();
    let press = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::default(),
    };
    frame(vec![egui::Event::PointerMoved(at), press(true)]);
    frame(vec![press(false)]);
    let opened = frame(Vec::new());
    texts(&opened).into_iter().map(|(text, _)| text).collect()
}

#[test]
fn shared_language_search_hint_is_rendered_in_the_ui_language() {
    let fixture = fixture();
    let case = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == "inline_node_editor_chrome_comes_from_language")
        .unwrap();
    for locale in case["locales"].as_array().unwrap() {
        let lang = locale["lang"].as_str().unwrap();
        let expected = locale["expected_language_search"].as_str().unwrap();
        let rendered = opened_language_popup_text(lang);
        assert!(
            rendered.iter().any(|text| text == expected),
            "{lang}: {expected:?} not in {rendered:?}"
        );
    }
}
