#[test]
fn saved_model_resolution_waits_for_matching_discovery() {
    use crate::config::types::{CustomModelDefinition, CustomModelType};
    let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/parity-fixtures/preset-system/saved-model-resolution.json"
    )))
    .unwrap();
    for case in fixture["pinned_cases"].as_array().unwrap() {
        let id = case["model_id"].as_str().unwrap();
        let block = crate::config::ProcessingBlock {
            model: id.to_string(),
            block_type: case["block_type"].as_str().unwrap().to_string(),
            ..Default::default()
        };
        let mut config = crate::config::Config {
            api_key: "test-key".to_string(),
            gemini_api_key: "test-key".to_string(),
            ..Default::default()
        };
        if let Some(resolved_type) = case["resolved_model_type"].as_str() {
            config.custom_models.push(CustomModelDefinition {
                id: id.to_string(),
                full_name: "example".to_string(),
                model_type: if resolved_type == "vision" {
                    CustomModelType::Vision
                } else {
                    CustomModelType::Text
                },
                ..Default::default()
            });
        }
        let resolved = super::resolve_block_model(&block, &config, false);
        if case["expected_action"] == "dispatch_selected" {
            assert_eq!(resolved.unwrap().id, id, "{}", case["name"]);
        } else {
            assert!(resolved.is_none(), "{}", case["name"]);
        }
        assert_eq!(block.model, id);
    }
}

#[test]
fn completed_output_does_not_resolve_a_replacement_model() {
    let block = crate::config::ProcessingBlock {
        model: "unavailable-model".to_string(),
        block_type: "text".to_string(),
        ..Default::default()
    };
    let config = crate::config::Config {
        api_key: "test-key".to_string(),
        gemini_api_key: "test-key".to_string(),
        ..Default::default()
    };
    assert!(super::resolve_block_model(&block, &config, true).is_none());
    assert!(super::resolve_block_model(&block, &config, false).is_none());
}
