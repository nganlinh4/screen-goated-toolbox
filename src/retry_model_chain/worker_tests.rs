use super::*;

#[test]
fn recorder_retry_respects_search_request_and_default_tool() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../parity-fixtures/preset-system/retry-runtime.json"
    ))
    .unwrap();
    let config = Config {
        api_key: "test-key".to_string(),
        ..Default::default()
    };
    for case in fixture["search_retry"]["compatibility_cases"]
        .as_array()
        .unwrap()
    {
        let mut model = get_model_by_id_with_custom("groq-gpt-oss-20b-text", &[]).unwrap();
        model.supports_search_override = Some(case["candidate_supports_search"].as_bool().unwrap());
        model.search_tool_enabled_by_default = case["candidate_search_tool_enabled_by_default"]
            .as_bool()
            .unwrap();
        assert_eq!(
            is_retry_candidate_compatible(
                &model,
                &ModelType::Text,
                case["search_enabled"].as_bool().unwrap(),
                &HashSet::new(),
                &config
            ),
            case["expected_eligible"].as_bool().unwrap(),
            "{}",
            case["name"]
        );
    }
}
