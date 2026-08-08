use super::{
    VisionTask, chain_ids, grounding_reports_not_visible, parse_box, parse_named_grounding_records,
    parse_open_grounding_records, parse_verification,
};
use crate::config::Config;

#[test]
fn grounding_chain_never_inherits_general_vision_models() {
    let mut config = Config::default();
    config.model_priority_chains.image_to_text = vec![
        "groq-qwen-3-6-27b-vision".into(),
        "google-gemma-4-26b-a4b-vision".into(),
    ];
    let grounding = chain_ids(&config, &[], VisionTask::Grounding);
    assert!(!grounding.is_empty());
    assert!(
        !grounding.iter().any(|id| {
            id == "groq-qwen-3-6-27b-vision" || id == "google-gemma-4-26b-a4b-vision"
        })
    );
    assert_eq!(
        chain_ids(&config, &[], VisionTask::General),
        ["groq-qwen-3-6-27b-vision", "google-gemma-4-26b-a4b-vision"]
    );
    assert_eq!(
        grounding,
        chain_ids(
            &config,
            &["groq-qwen-3-6-27b-vision"],
            VisionTask::Grounding
        ),
        "a preferred general model must not enter the grounding chain"
    );
}

#[test]
fn grounding_chain_matches_phone_control_fixture() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/parity-fixtures/phone-control/model-chain.json"
    )))
    .expect("Phone Control model-chain fixture parses");
    let expected = fixture["grounding"]["models"]
        .as_array()
        .expect("grounding models must be an array")
        .iter()
        .map(|model| model.as_str().expect("model id must be a string"))
        .collect::<Vec<_>>();
    assert_eq!(
        crate::model_config::COMPUTER_CONTROL_GROUNDING_MODEL_CHAIN_IDS,
        expected
    );
}

#[test]
fn structured_grounding_results_distinguish_terminal_from_malformed() {
    assert!(grounding_reports_not_visible(
        r#"{"points":[],"missing":["target"]}"#,
        &["target"]
    ));
    assert!(grounding_reports_not_visible(
        r#"{"points":[{"id":"from","x":100,"y":200,"label":"source"}],"missing":["to"]}"#,
        &["from", "to"],
    ));
    assert!(grounding_reports_not_visible(
        r#"{"points":[],"missing":["from","to"]}"#,
        &["from", "to"],
    ));
    assert!(!grounding_reports_not_visible(
        r#"{"points":[{"id":"target","x":100,"y":200,"label":"source"}],"missing":["to"]}"#,
        &["target"],
    ));
    assert!(!grounding_reports_not_visible(
        r#"{"points":[],"missing":["from","from"]}"#,
        &["from", "to"],
    ));
    assert!(!grounding_reports_not_visible("not json", &["target"]));
    assert!(
        parse_verification(r#"{"matches":false,"confidence":82,"what":"background"}"#).is_some()
    );
    assert!(parse_verification("not json").is_none());
}

#[test]
fn parses_box_2d_ignoring_the_key_digit() {
    let box_2d = parse_box(r#"{"box_2d": [100, 200, 300, 460]}"#).unwrap();
    assert_eq!(box_2d, [100.0, 200.0, 300.0, 460.0]);
}

#[test]
fn parses_bare_box_array() {
    assert_eq!(
        parse_box("```json\n[10, 20, 30, 40]\n```").unwrap(),
        [10.0, 20.0, 30.0, 40.0]
    );
}

#[test]
fn rejects_box_not_visible() {
    assert_eq!(parse_box(r#"{"error": "not visible"}"#), None);
}

#[test]
fn mark_records_accept_strict_json_and_preserve_reading_order() {
    let points = parse_open_grounding_records(
        r#"{"points":[{"x":900,"y":500,"label":"right target"},{"x":100,"y":200,"label":"left target"}]}"#,
    )
    .unwrap();
    assert_eq!(points.len(), 2);
    assert_eq!((points[0].x, points[0].y), (900.0, 500.0));
    assert_eq!((points[1].x, points[1].y), (100.0, 200.0));
}

#[test]
fn mark_records_reject_malformed_out_of_range_and_duplicates() {
    assert_eq!(parse_open_grounding_records("not records"), None);
    assert_eq!(
        parse_open_grounding_records(r#"{"points":[{"x":-1,"y":1001,"label":"target"}]}"#),
        None
    );
    assert_eq!(
        parse_open_grounding_records(
            r#"{"points":[{"x":100,"y":200,"label":"first"},{"x":104,"y":204,"label":"second"}]}"#,
        ),
        None
    );
}

#[test]
fn mark_records_have_a_hard_cap() {
    let body = serde_json::json!({
        "points": (0..31).map(|index| serde_json::json!({
            "x": index * 30,
            "y": index * 30,
            "label": format!("target {index}")
        })).collect::<Vec<_>>()
    })
    .to_string();
    assert_eq!(parse_open_grounding_records(&body), None);
}

#[test]
fn named_drag_records_require_every_exact_endpoint() {
    let points = parse_named_grounding_records(
        r#"{"points":[{"id":"from","x":100,"y":200,"label":"source"},{"id":"to","x":800,"y":700,"label":"destination"}],"missing":[]}"#,
        &["from", "to"],
    )
    .unwrap();
    assert_eq!(points.len(), 2);
    assert_eq!(
        parse_named_grounding_records(
            r#"{"points":[{"id":"from","x":100,"y":200,"label":"source"}],"missing":["to"]}"#,
            &["from", "to"]
        ),
        None
    );
}
