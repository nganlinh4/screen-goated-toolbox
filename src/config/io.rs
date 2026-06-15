//! Config I/O operations: load, save, and language utilities.

use std::path::PathBuf;
use std::sync::LazyLock;

use crate::config::config::Config;
use crate::config::preset::{Preset, ProcessingBlock, get_default_presets};
use crate::model_config::{ModelType, get_model_by_id_with_custom, model_is_non_llm};

// ============================================================================
// CONFIG PATH
// ============================================================================

/// Get the config file path
pub fn get_config_path() -> PathBuf {
    let config_dir = crate::paths::app_config_dir();
    let _ = std::fs::create_dir_all(&config_dir);
    config_dir.join("config_v3.json")
}

// ============================================================================
// CONFIG LOADING
// ============================================================================

/// Load config from disk, merging with defaults as needed
pub fn load_config() -> Config {
    let path = get_config_path();

    if !path.exists() {
        return Config::default();
    }

    let data = match std::fs::read_to_string(&path) {
        Ok(d) => d,
        Err(_) => return Config::default(),
    };

    let mut config: Config = match serde_json::from_str(&data) {
        Ok(c) => c,
        Err(e) => {
            // A hard parse failure means real corruption (migrate_config tolerates
            // field drift via #[serde(default)]). Preserve the offending file before
            // falling back to defaults, so an interrupted/corrupt write never silently
            // discards every preset, profile and API key.
            backup_corrupt_config(&path, &e);
            return Config::default();
        }
    };

    // Apply migrations and merge new defaults
    migrate_config(&mut config);

    config
}

/// Copy a config file that failed to parse to a timestamped `.corrupt-*` sibling
/// so the user's data can be recovered, instead of silently overwriting it.
fn backup_corrupt_config(path: &std::path::Path, err: &serde_json::Error) {
    let ts = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let mut backup = path.as_os_str().to_owned();
    backup.push(format!(".corrupt-{ts}"));
    let backup = PathBuf::from(backup);
    let copied = std::fs::copy(path, &backup).is_ok();
    crate::log_info!(
        "[config] config parse failed ({err}); {} -> {}",
        if copied {
            "preserved corrupt file at"
        } else {
            "FAILED to back up corrupt file to"
        },
        backup.display()
    );
}

/// Apply config migrations and merge new default presets
fn migrate_config(config: &mut Config) {
    let default_presets = get_default_presets();

    config.ensure_preset_profiles();
    migrate_preset_list(&mut config.presets, &default_presets);
    for profile in &mut config.preset_profiles {
        migrate_preset_list(&mut profile.presets, &default_presets);
        profile.active_preset_idx = profile
            .active_preset_idx
            .min(profile.presets.len().saturating_sub(1));
    }

    let custom_models = config.custom_models.clone();

    normalize_model_priority_chain(
        &mut config.model_priority_chains.image_to_text,
        ModelType::Vision,
        &custom_models,
    );
    normalize_saved_block_model_ids(&mut config.presets, &custom_models);
    for profile in &mut config.preset_profiles {
        normalize_saved_block_model_ids(&mut profile.presets, &custom_models);
    }
    normalize_model_priority_chain(
        &mut config.model_priority_chains.text_to_text,
        ModelType::Text,
        &custom_models,
    );
    normalize_translation_gummy_settings(config);
    normalize_removed_tts_methods(config);

    if config.realtime_translation_model == "taalas-rt" {
        config.realtime_translation_model =
            crate::model_config::REALTIME_TRANSLATION_MODEL_LLM.to_string();
    }

    config.sync_active_profile_from_presets();
}

fn normalize_removed_tts_methods(config: &mut Config) {
    if config.tts_method == crate::config::TtsMethod::FishAudioS2Pro {
        config.tts_method = crate::config::TtsMethod::GeminiLive;
    }
    if config.tts_playground.method == crate::config::TtsMethod::FishAudioS2Pro {
        config.tts_playground.method = crate::config::TtsMethod::GeminiLive;
    }
}

fn migrate_preset_list(presets: &mut Vec<Preset>, default_presets: &[Preset]) {
    // This ensures users get new presets from updates without losing their
    // custom presets or modifications to existing presets.
    let existing_ids: std::collections::HashSet<String> =
        presets.iter().map(|p| p.id.clone()).collect();

    let new_presets: Vec<Preset> = default_presets
        .iter()
        .filter(|p| p.is_builtin() && !existing_ids.contains(&p.id))
        .cloned()
        .collect();

    if !new_presets.is_empty() {
        presets.extend(new_presets);
    }

    for preset in presets.iter_mut() {
        if !preset.is_builtin() {
            continue;
        }

        if let Some(default_preset) = default_presets.iter().find(|p| p.id == preset.id) {
            preset.auto_paste = default_preset.auto_paste;
            preset.auto_paste_newline = default_preset.auto_paste_newline;
            preset.prompt_mode = default_preset.prompt_mode.clone();

            if preset.preset_type == "audio" {
                preset.auto_stop_recording = default_preset.auto_stop_recording;
            }
        }
    }

    for preset in presets.iter_mut() {
        if preset.blocks.is_empty() && !preset.is_master {
            preset.blocks.push(ProcessingBlock {
                block_type: preset.preset_type.clone(),
                ..Default::default()
            });
        }
    }
}

fn normalize_saved_block_model_ids(
    presets: &mut [Preset],
    custom_models: &[crate::config::types::CustomModelDefinition],
) {
    for preset in presets {
        for block in &mut preset.blocks {
            block.model =
                normalize_model_id_for_block(&block.block_type, &block.model, custom_models);
        }
    }
}

fn normalize_model_id_for_block(
    block_type: &str,
    model_id: &str,
    custom_models: &[crate::config::types::CustomModelDefinition],
) -> String {
    let expected_type = match block_type {
        "image" => Some(ModelType::Vision),
        "text" => Some(ModelType::Text),
        "audio" => Some(ModelType::Audio),
        _ => None,
    };

    let Some(expected_type) = expected_type else {
        return model_id.to_string();
    };

    if let Some(model) = get_model_by_id_with_custom(model_id, custom_models)
        && model.model_type == expected_type
    {
        return model_id.to_string();
    }

    default_model_id_for_type(expected_type).to_string()
}

fn normalize_model_priority_chain(
    chain: &mut Vec<String>,
    expected_type: ModelType,
    custom_models: &[crate::config::types::CustomModelDefinition],
) {
    let fallback = default_model_id_for_type(expected_type).to_string();
    let mut normalized = Vec::with_capacity(chain.len());
    let mut seen = std::collections::HashSet::new();

    for model_id in chain.drain(..) {
        let candidate = match get_model_by_id_with_custom(&model_id, custom_models) {
            Some(model) if model.model_type == expected_type && !model_is_non_llm(&model_id) => {
                model_id
            }
            _ => fallback.clone(),
        };

        if seen.insert(candidate.clone()) {
            normalized.push(candidate);
        }
    }

    if normalized.is_empty() {
        normalized.push(fallback);
    }

    *chain = normalized;
}

fn default_model_id_for_type(expected_type: ModelType) -> &'static str {
    match expected_type {
        ModelType::Vision => crate::model_config::DEFAULT_IMAGE_MODEL_ID,
        ModelType::Text => crate::model_config::DEFAULT_TEXT_MODEL_ID,
        ModelType::Audio => crate::model_config::PRESET_AUDIO_TRANSCRIBE_MODEL_ID,
    }
}

fn normalize_translation_gummy_settings(config: &mut Config) {
    let defaults = crate::config::TranslationGummySettings::default();
    if config.translation_gummy.first.language.trim().is_empty() {
        config.translation_gummy.first = defaults.first;
    }
    if config.translation_gummy.second.language.trim().is_empty() {
        config.translation_gummy.second = defaults.second;
    }
}

#[cfg(test)]
mod tests {
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

        // "gemini-3.1-flash-lite-preview" is a valid Vision model, so migration
        // preserves it for both builtin and custom presets (only invalid models
        // fall back to the default — see migrate_config_falls_back_for_missing_block_models).
        assert_eq!(
            config.presets[0].blocks[0].model,
            "gemini-3.1-flash-lite-preview"
        );
        assert_eq!(
            config.presets[1].blocks[0].model,
            "gemini-3.1-flash-lite-preview"
        );
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
                "gemini-3.1-flash-lite-preview".to_string(),
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
}

// ============================================================================
// CONFIG SAVING
// ============================================================================

/// Save config to disk atomically (temp + rename), so an interrupted write
/// never truncates the single file that holds every preset, profile and API key.
pub fn save_config(config: &Config) {
    let path = get_config_path();
    let mut config_to_save = config.clone();
    config_to_save.sync_active_profile_from_presets();
    if let Err(e) = crate::atomic_json::write_json_atomic(&path, &config_to_save) {
        crate::log_info!("[config] failed to save config: {e}");
    }
}

// ============================================================================
// LANGUAGE UTILITIES
// ============================================================================

/// All available language names (sorted, deduplicated)
static ALL_LANGUAGES: LazyLock<Vec<String>> = LazyLock::new(|| {
    let mut languages = Vec::new();
    for i in 0..10000 {
        if let Some(lang) = isolang::Language::from_usize(i) {
            // Only include languages with ISO 639-1 codes (major languages)
            if lang.to_639_1().is_some() {
                languages.push(lang.to_name().to_string());
            }
        }
    }
    languages.sort();
    languages.dedup();
    languages
});

/// Get all available language names
pub fn get_all_languages() -> &'static Vec<String> {
    &*ALL_LANGUAGES
}
