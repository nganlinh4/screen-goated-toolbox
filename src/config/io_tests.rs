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
            model: "qrserver-qr-scanner-vision".to_string(),
            ..Default::default()
        }],
        ..Default::default()
    };

    let mut config = legacy_config_with_presets(vec![custom]);

    migrate_config(&mut config);

    assert_eq!(
        config.presets[0].blocks[0].model,
        "qrserver-qr-scanner-vision"
    );
}

#[test]
fn migrate_config_preserves_valid_gemini_image_blocks() {
    let builtin = Preset {
        id: "preset_translate".to_string(),
        blocks: vec![ProcessingBlock {
            block_type: "image".to_string(),
            model: "google-gemini-3-1-flash-lite-vision".to_string(),
            ..Default::default()
        }],
        ..Default::default()
    };

    let custom = Preset {
        id: "custom_image_preset".to_string(),
        blocks: vec![ProcessingBlock {
            block_type: "image".to_string(),
            model: "google-gemini-3-1-flash-lite-vision".to_string(),
            ..Default::default()
        }],
        ..Default::default()
    };

    let mut config = legacy_config_with_presets(vec![builtin, custom]);

    migrate_config(&mut config);

    assert_eq!(
        config.presets[0].blocks[0].model,
        "google-gemini-3-1-flash-lite-vision"
    );
    assert_eq!(
        config.presets[1].blocks[0].model,
        "google-gemini-3-1-flash-lite-vision"
    );
}

#[test]
fn migrate_config_sanitizes_model_priority_chains() {
    let mut config = Config::default();
    config.model_priority_chains.image_to_text = vec![
        "google-gemini-3-1-flash-lite-vision".to_string(),
        "google-gtx-translate-text".to_string(),
        "missing-model".to_string(),
        "groq-qwen-3-6-27b-vision".to_string(),
    ];
    config.model_priority_chains.text_to_text = vec![
        "retired_text_model".to_string(),
        "google-gemma-4-26b-a4b-text".to_string(),
        "qrserver-qr-scanner-vision".to_string(),
        "groq-llama-3-3-70b-text".to_string(),
    ];

    migrate_config(&mut config);

    assert_eq!(
        config.model_priority_chains.image_to_text,
        vec![
            "google-gemini-3-1-flash-lite-vision".to_string(),
            crate::model_config::DEFAULT_IMAGE_MODEL_ID.to_string(),
            "groq-qwen-3-6-27b-vision".to_string()
        ]
    );
    assert_eq!(
        config.model_priority_chains.text_to_text,
        vec![
            crate::model_config::DEFAULT_TEXT_MODEL_ID.to_string(),
            "google-gemma-4-26b-a4b-text".to_string(),
            "groq-llama-3-3-70b-text".to_string()
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
fn migrate_config_moves_computer_control_out_of_every_profile() {
    let normal = |id: &str| Preset {
        id: id.to_string(),
        name: id.to_string(),
        ..Default::default()
    };
    let legacy = |hotkeys: Vec<Hotkey>| Preset {
        id: "preset_computer_control".to_string(),
        name: "Computer Control".to_string(),
        hotkeys,
        ..Default::default()
    };

    let profile_key = Hotkey::new(0x70, "F1", 0);
    let second_profile_key = Hotkey::new(0x71, "F2", 0);
    let mirror_only = Hotkey::new(0x72, "F3", 0);
    let existing = Hotkey::new(0x73, "F4", 0);
    let first = PresetProfile::new_default(
        vec![
            normal("before"),
            legacy(vec![profile_key.clone()]),
            normal("after"),
        ],
        2,
    );
    let second = PresetProfile::new_default(
        vec![
            legacy(vec![profile_key.clone(), second_profile_key.clone()]),
            normal("other"),
        ],
        0,
    );
    let mut active_mirror = first.presets.clone();
    active_mirror[1].hotkeys.push(mirror_only.clone());
    let mut config = Config {
        presets: active_mirror,
        active_preset_idx: first.active_preset_idx,
        preset_profiles: vec![first, second],
        computer_control_hotkeys: vec![existing.clone()],
        ..Default::default()
    };

    migrate_config(&mut config);

    assert!(
        config
            .presets
            .iter()
            .all(|preset| preset.id != "preset_computer_control")
    );
    assert!(config.preset_profiles.iter().all(|profile| {
        profile
            .presets
            .iter()
            .all(|preset| preset.id != "preset_computer_control")
    }));
    assert_eq!(config.active_preset_idx, 1);
    assert_eq!(config.preset_profiles[0].active_preset_idx, 1);
    assert_eq!(config.preset_profiles[1].active_preset_idx, 0);
    assert_eq!(
        config.computer_control_hotkeys,
        vec![existing, profile_key, mirror_only, second_profile_key]
    );
}

#[test]
fn default_presets_do_not_include_computer_control() {
    assert!(
        crate::config::preset::get_default_presets()
            .iter()
            .all(|preset| preset.id != "preset_computer_control")
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
