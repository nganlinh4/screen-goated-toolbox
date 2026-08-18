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
fn openrouter_is_enabled_for_new_and_missing_field_configs() {
    let defaults = Config::default();
    assert_eq!(defaults.use_groq, crate::model_config::DEFAULT_USE_GROQ);
    assert_eq!(defaults.use_gemini, crate::model_config::DEFAULT_USE_GEMINI);
    assert_eq!(
        defaults.use_openrouter,
        crate::model_config::DEFAULT_USE_OPENROUTER
    );
    assert_eq!(defaults.use_ollama, crate::model_config::DEFAULT_USE_OLLAMA);
    assert!(defaults.use_openrouter);

    let mut serialized = serde_json::to_value(defaults).unwrap();
    serialized.as_object_mut().unwrap().remove("use_openrouter");
    let restored: Config = serde_json::from_value(serialized).unwrap();

    assert!(restored.use_openrouter);
}

#[test]
fn startup_animation_is_enabled_for_new_and_missing_field_configs() {
    let defaults = Config::default();
    assert!(defaults.show_startup_animation);

    let mut serialized = serde_json::to_value(defaults).unwrap();
    serialized
        .as_object_mut()
        .unwrap()
        .remove("show_startup_animation");
    let restored: Config = serde_json::from_value(serialized).unwrap();

    assert!(restored.show_startup_animation);
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
        "groq-gpt-oss-120b-text".to_string(),
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
            "groq-gpt-oss-120b-text".to_string()
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
fn migrate_config_preserves_user_edited_builtin_settings() {
    let defaults = crate::config::preset::get_default_presets();
    let mut image = defaults
        .iter()
        .find(|preset| preset.id == "preset_translate")
        .unwrap()
        .clone();
    image.auto_paste = !image.auto_paste;
    image.auto_paste_newline = !image.auto_paste_newline;
    image.prompt_mode = "user-selected-mode".to_string();

    let mut audio = defaults
        .iter()
        .find(|preset| preset.id == "preset_transcribe")
        .unwrap()
        .clone();
    audio.auto_stop_recording = !audio.auto_stop_recording;

    let expected_image = (
        image.auto_paste,
        image.auto_paste_newline,
        image.prompt_mode.clone(),
    );
    let expected_audio_auto_stop = audio.auto_stop_recording;
    let mut config = legacy_config_with_presets(vec![image, audio]);

    migrate_config(&mut config);

    let image = config
        .presets
        .iter()
        .find(|preset| preset.id == "preset_translate")
        .unwrap();
    assert_eq!(
        (
            image.auto_paste,
            image.auto_paste_newline,
            image.prompt_mode.clone(),
        ),
        expected_image
    );
    let audio = config
        .presets
        .iter()
        .find(|preset| preset.id == "preset_transcribe")
        .unwrap();
    assert_eq!(audio.auto_stop_recording, expected_audio_auto_stop);
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
fn retired_builtin_migrates_to_its_replacement_in_every_profile() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/parity-fixtures/preset-system/catalog-overrides.json"
    )))
    .unwrap();
    let retirement = &fixture["retired_builtins"][0];
    let retired_id = retirement["preset_id"].as_str().unwrap();
    let replacement_id = retirement["replacement_id"].as_str().unwrap();
    let default_code = retirement["windows_default_hotkey"]["code"]
        .as_u64()
        .unwrap() as u32;
    let default_modifiers = retirement["windows_default_hotkey"]["modifiers"]
        .as_u64()
        .unwrap() as u32;
    assert!(retirement["transfer_unique_hotkeys"].as_bool().unwrap());
    assert!(retirement["transfer_favorite"].as_bool().unwrap());
    assert!(retirement["redirect_active_selection"].as_bool().unwrap());

    let preset = |id: &str, hotkeys: Vec<Hotkey>, is_favorite: bool| Preset {
        id: id.to_string(),
        name: id.to_string(),
        hotkeys,
        is_favorite,
        ..Default::default()
    };
    let duplicate = Hotkey::new(0x70, "F1", 0);
    let backtick = Hotkey::new(default_code, "Backtick", default_modifiers);
    let first = PresetProfile::new_default(
        vec![
            preset(replacement_id, vec![duplicate.clone()], false),
            preset(retired_id, vec![backtick.clone(), duplicate.clone()], true),
        ],
        1,
    );
    let second_key = Hotkey::new(0x71, "F2", 0);
    let second = PresetProfile::new_default(
        vec![
            preset("before", Vec::new(), false),
            preset(retired_id, vec![second_key.clone()], false),
            preset(replacement_id, Vec::new(), false),
        ],
        1,
    );
    let mut config = Config {
        presets: first.presets.clone(),
        active_preset_idx: first.active_preset_idx,
        preset_profiles: vec![first, second],
        ..Default::default()
    };
    let priority_chains = config.model_priority_chains.clone();

    migrate_config(&mut config);

    assert_eq!(config.model_priority_chains, priority_chains);
    assert!(config.presets.iter().all(|preset| preset.id != retired_id));
    assert!(
        config
            .preset_profiles
            .iter()
            .all(|profile| { profile.presets.iter().all(|preset| preset.id != retired_id) })
    );
    assert_eq!(config.presets[config.active_preset_idx].id, replacement_id);
    assert_eq!(
        config.preset_profiles[0].presets[config.preset_profiles[0].active_preset_idx].id,
        replacement_id
    );
    assert_eq!(
        config.preset_profiles[1].presets[config.preset_profiles[1].active_preset_idx].id,
        replacement_id
    );

    let translated = config
        .presets
        .iter()
        .find(|preset| preset.id == replacement_id)
        .unwrap();
    assert!(translated.is_favorite);
    assert_eq!(translated.hotkeys, vec![duplicate, backtick]);
    let second_translated = config.preset_profiles[1]
        .presets
        .iter()
        .find(|preset| preset.id == replacement_id)
        .unwrap();
    assert_eq!(second_translated.hotkeys, vec![second_key]);

    let defaults = crate::config::preset::get_default_presets();
    assert!(defaults.iter().all(|preset| preset.id != retired_id));
    let translated_default = defaults
        .iter()
        .find(|preset| preset.id == replacement_id)
        .unwrap();
    assert!(
        translated_default
            .hotkeys
            .iter()
            .any(|hotkey| { hotkey.code == default_code && hotkey.modifiers == default_modifiers })
    );
}

#[test]
fn accurate_retranslate_retirement_uses_the_standard_retranslate_chain() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/parity-fixtures/preset-system/catalog-overrides.json"
    )))
    .unwrap();
    let retirement = fixture["retired_builtins"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["preset_id"] == "preset_extract_retrans_retrans")
        .unwrap();
    let retired_id = retirement["preset_id"].as_str().unwrap();
    let replacement_id = retirement["replacement_id"].as_str().unwrap();
    assert!(retirement["transfer_unique_hotkeys"].as_bool().unwrap());
    assert!(retirement["transfer_favorite"].as_bool().unwrap());
    assert!(retirement["redirect_active_selection"].as_bool().unwrap());

    let migrated_hotkey = Hotkey::new(0x72, "F3", 0);
    let mut config = legacy_config_with_presets(vec![
        Preset {
            id: replacement_id.to_string(),
            ..Default::default()
        },
        Preset {
            id: retired_id.to_string(),
            hotkeys: vec![migrated_hotkey.clone()],
            is_favorite: true,
            ..Default::default()
        },
    ]);
    config.active_preset_idx = 1;
    let priority_chains = config.model_priority_chains.clone();

    migrate_config(&mut config);

    assert_eq!(config.model_priority_chains, priority_chains);
    assert!(config.presets.iter().all(|preset| preset.id != retired_id));
    assert!(
        config.preset_profiles[0]
            .presets
            .iter()
            .all(|preset| preset.id != retired_id)
    );
    assert_eq!(config.presets[config.active_preset_idx].id, replacement_id);
    let replacement = config
        .presets
        .iter()
        .find(|preset| preset.id == replacement_id)
        .unwrap();
    assert!(replacement.is_favorite);
    assert_eq!(replacement.hotkeys, vec![migrated_hotkey]);
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
