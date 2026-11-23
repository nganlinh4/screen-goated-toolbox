use super::{Preset, ProcessingBlock, migrate_config};
use crate::config::Config;
use crate::config::types::PresetProfile;

#[test]
fn saved_models_and_chains_survive_migration_and_reload() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../parity-fixtures/preset-system/saved-model-resolution.json"
    ))
    .unwrap();
    let cases = fixture["pinned_cases"].as_array().unwrap();
    let presets: Vec<Preset> = cases
        .iter()
        .enumerate()
        .map(|(i, case)| Preset {
            id: format!("saved-model-test-{i}"),
            blocks: vec![ProcessingBlock {
                block_type: case["block_type"].as_str().unwrap().to_string(),
                model: case["model_id"].as_str().unwrap().to_string(),
                ..Default::default()
            }],
            ..Default::default()
        })
        .collect();
    let profile = PresetProfile::new_default(presets.clone(), 0);
    let mut config = Config {
        presets: presets.clone(),
        preset_profiles: vec![profile.clone(), profile],
        ..Default::default()
    };
    let chain = fixture["chain_cases"][0]["authored"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    let expected = fixture["chain_cases"][0]["expected_saved"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    config.model_priority_chains.text_to_text = chain.clone();
    config.model_priority_chains.image_to_text = chain;
    for _ in 0..2 {
        migrate_config(&mut config);
        for (i, case) in cases.iter().enumerate() {
            let id = format!("saved-model-test-{i}");
            let expected_id = case["expected_saved_id"].as_str().unwrap();
            assert_eq!(
                config
                    .presets
                    .iter()
                    .find(|preset| preset.id == id)
                    .unwrap()
                    .blocks[0]
                    .model,
                expected_id
            );
            for profile in &config.preset_profiles {
                assert_eq!(
                    profile
                        .presets
                        .iter()
                        .find(|preset| preset.id == id)
                        .unwrap()
                        .blocks[0]
                        .model,
                    expected_id
                );
            }
        }
        assert_eq!(config.model_priority_chains.text_to_text, expected);
        assert_eq!(config.model_priority_chains.image_to_text, expected);
        config = serde_json::from_str(&serde_json::to_string(&config).unwrap()).unwrap();
    }
}
