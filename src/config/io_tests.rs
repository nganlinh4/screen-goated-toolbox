use super::migrate_config;
use crate::config::types::PresetProfile;
use crate::config::{Config, Hotkey, Preset, ProcessingBlock};

fn legacy_config_with_presets(presets: Vec<Preset>) -> Config {
    Config {
        presets,
        preset_profiles: Vec::new(),
        active_preset_profile_idx: 0,
        ..Default::default()
    }
}

#[test]
fn migrate_config_falls_back_for_missing_block_models() {
    let builtin = Preset {
        id: "preset_translate".to_string(),
        blocks: vec![ProcessingBlock {
            block_type: "image".to_string(),
            model: "maverick".to_string(),
            ..Default::default()
        }],
        ..Default::default()
    };

    let custom = Preset {
        id: "custom_image_preset".to_string(),
        blocks: vec![ProcessingBlock {
            block_type: "text".to_string(),
            model: "retired_text_model".to_string(),
            ..Default::default()
        }],
        ..Default::default()
    };

    let mut config = legacy_config_with_presets(vec![builtin, custom]);

    migrate_config(&mut config);

    assert_eq!(
        config.presets[0].blocks[0].model,
        crate::model_config::DEFAULT_IMAGE_MODEL_ID
    );
    assert_eq!(
        config.presets[1].blocks[0].model,
        crate::model_config::DEFAULT_TEXT_MODEL_ID
    );
}

#[test]
fn migrate_config_preserves_valid_non_llm_image_blocks() {
    let custom = Preset {
        id: "custom_image_preset".to_string(),
        blocks: vec![ProcessingBlock {
            block_type: "image".to_string(),
            model: "qr-scanner".to_string(),
            ..Default::default()
        }],
        ..Default::default()
    };

    let mut config = legacy_config_with_presets(vec![custom]);

    migrate_config(&mut config);

    assert_eq!(config.presets[0].blocks[0].model, "qr-scanner");
}

#[test]
fn migrate_config_preserves_valid_gemini_image_blocks() {
    let builtin = Preset {
        id: "preset_translate".to_string(),
        blocks: vec![ProcessingBlock {
            block_type: "image".to_string(),
            model: "gemini-3.1-flash-lite-preview".to_string(),
            ..Default::default()
        }],
        ..Default::default()
    };

    let custom = Preset {
        id: "custom_image_preset".to_string(),
        blocks: vec![ProcessingBlock {
            block_type: "image".to_string(),
            model: "gemini-3.1-flash-lite-preview".to_string(),
            ..Default::default()
        }],
        ..Default::default()
    };

    let mut config = legacy_config_with_presets(vec![builtin, custom]);

    migrate_config(&mut config);

    assert_eq!(config.presets[0].blocks[0].model, "gemini-3.1-flash-lite");
    assert_eq!(config.presets[1].blocks[0].model, "gemini-3.1-flash-lite");
}

#[test]
fn migrate_config_promotes_old_builtin_image_defaults_only() {
    let builtin = Preset {
        id: "preset_ocr".to_string(),
        blocks: vec![ProcessingBlock {
            block_type: "image".to_string(),
            model: "scout".to_string(),
            ..Default::default()
        }],
        ..Default::default()
    };
    let custom = Preset {
        id: "custom_scout".to_string(),
        blocks: vec![ProcessingBlock {
            block_type: "image".to_string(),
            model: "scout".to_string(),
            ..Default::default()
        }],
        ..Default::default()
    };
    let mut config = legacy_config_with_presets(vec![builtin, custom]);

    migrate_config(&mut config);

    assert_eq!(
        config.presets[0].blocks[0].model,
        "gemma-4-31b-cerebras-vision"
    );
    assert_eq!(config.presets[1].blocks[0].model, "scout");
}

#[test]
fn migrate_config_sanitizes_model_priority_chains() {
    let mut config = Config::default();
    config.model_priority_chains.image_to_text = vec![
        "gemini-3.1-flash-lite-preview".to_string(),
        "google-gtx".to_string(),
        "missing-model".to_string(),
        "scout".to_string(),
    ];
    config.model_priority_chains.text_to_text = vec![
        "retired_text_model".to_string(),
        "gemma-4-26b-a4b".to_string(),
        "qr-scanner".to_string(),
        "scout".to_string(),
        "text_llama_3_3_70b".to_string(),
    ];

    migrate_config(&mut config);

    assert_eq!(
        config.model_priority_chains.image_to_text,
        vec![
            "gemini-3.1-flash-lite".to_string(),
            "gemma-4-31b-cerebras-vision".to_string(),
            "scout".to_string()
        ]
    );
    assert_eq!(
        config.model_priority_chains.text_to_text,
        vec![
            crate::model_config::DEFAULT_TEXT_MODEL_ID.to_string(),
            "gemma-4-26b-a4b".to_string(),
            "text_llama_3_3_70b".to_string()
        ]
    );
}

#[test]
fn migrate_config_falls_back_to_default_text_model_id() {
    let builtin = Preset {
        id: "preset_translate".to_string(),
        blocks: vec![ProcessingBlock {
            block_type: "text".to_string(),
            model: "retired_text_model".to_string(),
            ..Default::default()
        }],
        ..Default::default()
    };

    let custom = Preset {
        id: "custom_text_preset".to_string(),
        blocks: vec![ProcessingBlock {
            block_type: "text".to_string(),
            model: "retired_text_model".to_string(),
            ..Default::default()
        }],
        ..Default::default()
    };

    let mut config = legacy_config_with_presets(vec![builtin, custom]);

    config.model_priority_chains.text_to_text = vec!["retired_text_model".to_string()];

    migrate_config(&mut config);

    assert_eq!(
        config.presets[0].blocks[0].model,
        crate::model_config::DEFAULT_TEXT_MODEL_ID
    );
    assert_eq!(
        config.presets[1].blocks[0].model,
        crate::model_config::DEFAULT_TEXT_MODEL_ID
    );
    assert_eq!(
        config.model_priority_chains.text_to_text,
        vec![crate::model_config::DEFAULT_TEXT_MODEL_ID.to_string()]
    );
}

#[test]
fn migrate_config_fills_missing_translation_gummy_defaults() {
    let mut config = Config::default();
    config.translation_gummy.first.language.clear();
    config.translation_gummy.second.language.clear();
    config.translation_gummy.second.accent.clear();
    config.translation_gummy.second.tone.clear();

    migrate_config(&mut config);

    assert_eq!(config.translation_gummy.first.language, "English");
    assert_eq!(config.translation_gummy.first.accent, "");
    assert_eq!(config.translation_gummy.first.tone, "");
    assert_eq!(config.translation_gummy.second.language, "Korean");
    assert_eq!(config.translation_gummy.second.accent, "Busan");
    assert_eq!(config.translation_gummy.second.tone, "polite");
}

#[test]
fn migrate_config_creates_default_profile_for_legacy_presets() {
    let custom = Preset {
        id: "custom_legacy_preset".to_string(),
        name: "Legacy".to_string(),
        ..Default::default()
    };
    let mut config = legacy_config_with_presets(vec![custom]);

    migrate_config(&mut config);

    assert_eq!(config.preset_profiles.len(), 1);
    assert_eq!(config.preset_profiles[0].name, "Default");
    assert!(
        config.preset_profiles[0]
            .presets
            .iter()
            .any(|preset| preset.id == "custom_legacy_preset")
    );
}

#[test]
fn add_preset_profile_clones_active_preset_config() {
    let mut preset = Preset {
        id: "profile_source_preset".to_string(),
        name: "Profile Source".to_string(),
        is_favorite: true,
        ..Default::default()
    };
    preset.hotkeys.push(Hotkey::new(65, "A", 2));

    let mut config = legacy_config_with_presets(vec![preset]);
    migrate_config(&mut config);

    config.add_preset_profile_from_active();

    assert_eq!(config.preset_profiles.len(), 2);
    assert_eq!(config.active_preset_profile_idx, 1);
    assert_eq!(config.presets[0].id, "profile_source_preset");
    assert!(config.presets[0].is_favorite);
    assert_eq!(config.presets[0].hotkeys, vec![Hotkey::new(65, "A", 2)]);
}

#[test]
fn delete_preset_profile_selects_left_neighbor_for_active_only() {
    let first = PresetProfile::new_default(vec![Preset::default()], 0);
    let second = PresetProfile::new_default(
        vec![Preset {
            id: "second_profile_preset".to_string(),
            ..Default::default()
        }],
        0,
    );
    let third = PresetProfile::new_default(
        vec![Preset {
            id: "third_profile_preset".to_string(),
            ..Default::default()
        }],
        0,
    );
    let mut config = Config {
        preset_profiles: vec![first, second, third],
        active_preset_profile_idx: 1,
        ..Default::default()
    };
    config.ensure_preset_profiles();

    config.delete_preset_profile(2);
    assert_eq!(config.active_preset_profile_idx, 1);
    assert_eq!(config.presets[0].id, "second_profile_preset");

    config.delete_preset_profile(1);
    assert_eq!(config.active_preset_profile_idx, 0);
}
