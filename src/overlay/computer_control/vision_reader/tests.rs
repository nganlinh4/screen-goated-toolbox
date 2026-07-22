use super::{
    VisionTask, chain_ids, parse_box, parse_point, parse_points, parse_verification,
    response_reports_not_visible,
};
use crate::config::Config;

#[test]
fn grounding_chain_never_inherits_general_vision_models() {
    let mut config = Config::default();
    config.model_priority_chains.image_to_text = vec!["scout".into(), "gemini-flash".into()];
    let grounding = chain_ids(&config, &[], VisionTask::Grounding);
    assert!(!grounding.is_empty());
    assert!(
        !grounding
            .iter()
            .any(|id| id == "scout" || id == "gemini-flash")
    );
    assert_eq!(
        chain_ids(&config, &[], VisionTask::General),
        ["scout", "gemini-flash"]
    );
    assert_eq!(
        grounding,
        chain_ids(&config, &["scout"], VisionTask::Grounding),
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
    assert!(response_reports_not_visible(r#"{"error":"not visible"}"#));
    assert!(parse_verification(r#"{"matches":false,"confidence":82}"#).is_some());
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
fn parses_json_point() {
    assert_eq!(parse_point(r#"{"x": 420, "y": 680}"#), Some((420.0, 680.0)));
}

#[test]
fn parses_fenced_and_reordered() {
    let answer = "```json\n{ \"y\": 100, \"x\": 900 }\n```";
    assert_eq!(parse_point(answer), Some((900.0, 100.0)));
}

#[test]
fn rejects_not_visible() {
    assert_eq!(parse_point(r#"{"error": "not visible"}"#), None);
}

#[test]
fn verbose_reasoning_uses_final_coordinates() {
    let answer = "The grid starts at x=0 and y=0. Final: {\"x\": 150, \"y\": 250}.";
    assert_eq!(parse_point(answer), Some((150.0, 250.0)));
}

#[test]
fn point_array_accepts_empty_and_normalizes_order() {
    assert_eq!(parse_points("[]"), Some(Vec::new()));
    let points =
        parse_points(r#"[{"x":900,"y":500},{"x":100,"y":200},{"x":104,"y":204}]"#).unwrap();
    assert_eq!(points.len(), 2);
    assert_eq!((points[0].x, points[0].y), (100.0, 200.0));
    assert_eq!((points[1].x, points[1].y), (900.0, 500.0));
}

#[test]
fn point_array_rejects_malformed_or_out_of_range_only() {
    assert_eq!(parse_points("not json"), None);
    assert_eq!(parse_points(r#"[{"x":-1,"y":1001}]"#), None);
}

#[test]
fn point_array_has_a_hard_cap() {
    let body = (0..80)
        .map(|index| format!(r#"{{"x":{},"y":{}}}"#, index * 12, index * 12))
        .collect::<Vec<_>>()
        .join(",");
    assert_eq!(parse_points(&format!("[{body}]")).unwrap().len(), 30);
}
