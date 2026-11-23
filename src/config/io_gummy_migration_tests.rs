use super::migrate_config;
use crate::config::{Config, Hotkey, TranslationGummySettings};
use serde_json::Value;

fn identities(config: &Config) -> Vec<(u32, u32)> {
    config
        .translation_gummy
        .hotkeys
        .iter()
        .map(|binding| (binding.code, binding.modifiers))
        .collect()
}

#[test]
fn gummy_legacy_migration_preserves_bindings_before_registration_and_unrelated_save() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../parity-fixtures/translation-gummy/hotkey-migration.json"
    ))
    .unwrap();
    assert_eq!(fixture["version"], 1);
    assert_eq!(fixture["platform"], "windows");
    for case in fixture["cases"].as_array().unwrap() {
        let mut config: Config = serde_json::from_value(serde_json::json!({
            "translation_gummy": case["saved"],
            "screen_record_hotkeys": [],
            "presets": [],
            "preset_profiles": [],
            "screen_translate": { "hotkeys": [], "fullscreen_hotkeys": [], "subtitle_hotkeys": [] }
        }))
        .unwrap();
        let expected: Vec<Hotkey> = serde_json::from_value(case["expected"].clone()).unwrap();
        let expected_identity = expected
            .iter()
            .map(|binding| (binding.code, binding.modifiers))
            .collect::<Vec<_>>();

        migrate_config(&mut config);
        assert_eq!(identities(&config), expected_identity, "{}", case["name"]);
        assert!(config.translation_gummy.hotkey.is_none());
        for binding in &expected {
            assert!(matches!(
                config.check_hotkey_conflict(binding.code, binding.modifiers, None),
                Some(crate::config::HotkeyConflict::Global {
                    owner: crate::config::GlobalHotkeyOwner::TranslationGummy,
                    ..
                })
            ));
        }

        config.show_startup_animation = !config.show_startup_animation;
        let saved = serde_json::to_value(&config).unwrap();
        assert!(saved["translation_gummy"].get("hotkey").is_none());
        let mut reloaded: Config = serde_json::from_value(saved).unwrap();
        migrate_config(&mut reloaded);
        assert_eq!(
            identities(&reloaded),
            expected_identity,
            "{} save cycle",
            case["name"]
        );
        assert_eq!(
            reloaded.show_startup_animation,
            config.show_startup_animation
        );
        let once = reloaded.translation_gummy.clone();
        migrate_config(&mut reloaded);
        assert_eq!(
            reloaded.translation_gummy, once,
            "{} repeated migration",
            case["name"]
        );
    }
}

#[test]
fn gummy_legacy_migration_retains_blank_language_default_restoration() {
    let mut config = Config::default();
    config.translation_gummy.first.language = "  ".into();
    config.translation_gummy.first.accent = "unused".into();
    config.translation_gummy.second.language.clear();
    config.translation_gummy.second.tone = "unused".into();
    migrate_config(&mut config);
    let defaults = TranslationGummySettings::default();
    assert_eq!(config.translation_gummy.first, defaults.first);
    assert_eq!(config.translation_gummy.second, defaults.second);
}
