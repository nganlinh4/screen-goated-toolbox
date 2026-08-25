use super::migrate_config;
use crate::config::types::PresetProfile;
use crate::config::{Config, Hotkey, LiveTranslateInterface, Preset};

#[test]
fn migrate_config_moves_live_translate_out_of_every_profile() {
    let normal = |id: &str| Preset {
        id: id.to_string(),
        name: id.to_string(),
        ..Default::default()
    };
    let legacy = |hotkeys: Vec<Hotkey>, interface: &str| Preset {
        id: "preset_realtime_audio_translate".to_string(),
        name: "Live Translate".to_string(),
        hotkeys,
        legacy_realtime_window_mode: interface.to_string(),
        ..Default::default()
    };

    let existing = Hotkey::new(0x70, "F1", 0);
    let profile_key = Hotkey::new(0x71, "F2", 0);
    let mirror_only = Hotkey::new(0x72, "F3", 0);
    let second_profile_key = Hotkey::new(0x73, "F4", 0);
    let first = PresetProfile::new_default(
        vec![
            normal("before"),
            legacy(vec![existing.clone(), profile_key.clone()], "standard"),
            normal("after"),
        ],
        2,
    );
    let second = PresetProfile::new_default(
        vec![
            legacy(vec![second_profile_key.clone()], "standard"),
            normal("second"),
        ],
        0,
    );
    let mut active_mirror = first.presets.clone();
    active_mirror[1].legacy_realtime_window_mode = "minimal".to_string();
    active_mirror[1].hotkeys.push(mirror_only.clone());
    let mut config = Config {
        presets: active_mirror,
        active_preset_idx: first.active_preset_idx,
        preset_profiles: vec![first, second],
        live_translate: crate::config::types::LiveTranslateSettings {
            hotkeys: vec![existing.clone()],
            ..Default::default()
        },
        ..Default::default()
    };

    migrate_config(&mut config);

    assert!(
        config
            .presets
            .iter()
            .all(|preset| preset.id != "preset_realtime_audio_translate")
    );
    assert!(config.preset_profiles.iter().all(|profile| {
        profile
            .presets
            .iter()
            .all(|preset| preset.id != "preset_realtime_audio_translate")
    }));
    assert_eq!(config.active_preset_idx, 1);
    assert_eq!(config.preset_profiles[0].active_preset_idx, 1);
    assert_eq!(config.preset_profiles[1].active_preset_idx, 0);
    assert_eq!(
        config.live_translate.hotkeys,
        vec![existing, profile_key, mirror_only, second_profile_key]
    );
    assert_eq!(
        config.live_translate.interface,
        LiveTranslateInterface::Minimal
    );
}

#[test]
fn default_presets_do_not_include_live_translate() {
    assert!(
        crate::config::preset::get_default_presets()
            .iter()
            .all(|preset| preset.id != "preset_realtime_audio_translate")
    );
}

#[test]
fn preset_schema_does_not_serialize_live_translate_options() {
    let preset = Preset {
        legacy_realtime_window_mode: "minimal".to_string(),
        ..Default::default()
    };
    let serialized = serde_json::to_value(preset).unwrap();

    assert!(serialized.get("audio_processing_mode").is_none());
    assert!(serialized.get("realtime_window_mode").is_none());
}
