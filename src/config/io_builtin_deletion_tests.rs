use super::migrate_config;
use crate::config::Config;
use crate::config::types::PresetProfile;

fn builtin_ids(config: &Config) -> Vec<String> {
    config
        .presets
        .iter()
        .filter(|preset| preset.is_builtin())
        .map(|preset| preset.id.clone())
        .collect()
}

#[test]
fn deleted_builtins_stay_deleted_across_startups_in_every_profile() {
    let mut config = Config::default();
    migrate_config(&mut config);
    let all_builtins = builtin_ids(&config);
    assert!(all_builtins.len() > 2);
    let kept = all_builtins[0].clone();

    let curated: Vec<_> = config
        .presets
        .iter()
        .filter(|preset| !preset.is_builtin() || preset.id == kept)
        .cloned()
        .collect();
    config.presets = curated.clone();
    config.active_preset_idx = 0;
    config.preset_profiles = vec![
        PresetProfile::new_default(curated.clone(), 0),
        PresetProfile::new_default(curated, 0),
    ];

    let saved = serde_json::to_string(&config).unwrap();
    let mut reloaded: Config = serde_json::from_str(&saved).unwrap();
    migrate_config(&mut reloaded);

    assert_eq!(builtin_ids(&reloaded), vec![kept.clone()]);
    for profile in &reloaded.preset_profiles {
        let ids: Vec<_> = profile
            .presets
            .iter()
            .filter(|preset| preset.is_builtin())
            .map(|preset| preset.id.clone())
            .collect();
        assert_eq!(ids, vec![kept.clone()]);
    }
}

#[test]
fn builtins_new_since_the_last_startup_are_still_offered() {
    let mut config = Config::default();
    migrate_config(&mut config);
    let newest = config
        .offered_builtin_preset_ids
        .pop()
        .expect("built-in presets are recorded as offered");
    config.presets.retain(|preset| preset.id != newest);
    config.sync_active_profile_from_presets();

    migrate_config(&mut config);

    assert!(config.presets.iter().any(|preset| preset.id == newest));
    assert!(config.offered_builtin_preset_ids.contains(&newest));
}
