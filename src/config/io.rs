//! Config I/O operations: load, save, and language utilities.

use std::path::PathBuf;

use crate::config::config::Config;
use crate::config::preset::{Preset, ProcessingBlock, get_default_presets};
use crate::model_config::{ModelType, get_model_by_id, model_is_non_llm};

// ============================================================================
// CONFIG PATH
// ============================================================================

/// Get the config file path
pub fn get_config_path() -> PathBuf {
    let config_dir = dirs::config_dir()
        .unwrap_or_default()
        .join("screen-goated-toolbox");
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
        Err(_) => return Config::default(),
    };

    // Apply migrations and merge new defaults
    migrate_config(&mut config);

    config
}

/// Apply config migrations and merge new default presets
fn migrate_config(config: &mut Config) {
    let default_presets = get_default_presets();

    // -------------------------------------------------------------------------
    // 1. AUTO-MERGE NEW DEFAULT PRESETS
    // -------------------------------------------------------------------------
    // This ensures users get new presets from updates without losing their
    // custom presets or modifications to existing presets.
    //
    // Strategy:
    // - Default presets have IDs starting with "preset_"
    // - User-created presets have timestamp-based IDs
    // - For each default preset not in user's config → add it
    // - Keep user's version of existing presets (they may have customized)

    let existing_ids: std::collections::HashSet<String> =
        config.presets.iter().map(|p| p.id.clone()).collect();

    let new_presets: Vec<Preset> = default_presets
        .iter()
        .filter(|p| p.is_builtin() && !existing_ids.contains(&p.id))
        .cloned()
        .collect();

    if !new_presets.is_empty() {
        config.presets.extend(new_presets);
    }

    // -------------------------------------------------------------------------
    // 2. MIGRATE CRITICAL SETTINGS FOR EXISTING BUILT-IN PRESETS
    // -------------------------------------------------------------------------
    // When default presets are updated with new settings (like auto_paste=true),
    // sync those settings to existing user presets.

    for preset in &mut config.presets {
        if !preset.is_builtin() {
            continue;
        }

        if let Some(default_preset) = default_presets.iter().find(|p| p.id == preset.id) {
            // Sync auto_paste and auto_paste_newline
            preset.auto_paste = default_preset.auto_paste;
            preset.auto_paste_newline = default_preset.auto_paste_newline;

            // Sync prompt_mode (critical: determines whether text input appears for dynamic presets)
            preset.prompt_mode = default_preset.prompt_mode.clone();

            // Do not sync blocks from defaults here: built-in presets are user-editable,
            // and overwriting blocks would reset custom models/prompts/render modes.

            // Sync audio-specific settings
            if preset.preset_type == "audio" {
                preset.auto_stop_recording = default_preset.auto_stop_recording;
            }
        }
    }

    sanitize_model_priority_chain(
        &mut config.model_priority_chains.image_to_text,
        ModelType::Vision,
    );
    sanitize_model_priority_chain(
        &mut config.model_priority_chains.text_to_text,
        ModelType::Text,
    );

    // -------------------------------------------------------------------------
    // 3. MIGRATE RETIRED MODEL IDS IN SAVED PRESETS
    // -------------------------------------------------------------------------
    // Migrate any saved blocks (including user-custom presets) away from retired
    // model IDs without overwriting prompts or other block settings.
    for preset in &mut config.presets {
        for block in &mut preset.blocks {
            match block.model.as_str() {
                "cerebras_zai_glm_4_7" => {
                    block.model = crate::model_config::DEFAULT_CEREBRAS_TEXT_MODEL_ID.to_string();
                }
                "maverick" => {
                    block.model = crate::model_config::DEFAULT_IMAGE_MODEL_ID.to_string();
                }
                _ => {}
            }
        }
    }

    for preset in &mut config.presets {
        if !preset.is_builtin() {
            continue;
        }

        for block in &mut preset.blocks {
            if block.block_type == "image" && block.model == "gemini-3.1-flash-lite-preview" {
                block.model = crate::model_config::DEFAULT_IMAGE_MODEL_ID.to_string();
            }
        }
    }

    // -------------------------------------------------------------------------
    // 4. ENSURE EVERY PRESET HAS AT LEAST ONE BLOCK
    // -------------------------------------------------------------------------
    for preset in &mut config.presets {
        if preset.blocks.is_empty() && !preset.is_master {
            preset.blocks.push(ProcessingBlock {
                block_type: preset.preset_type.clone(),
                ..Default::default()
            });
        }
    }
}

fn sanitize_model_priority_chain(chain: &mut Vec<String>, expected_type: ModelType) {
    chain.retain(|model_id| {
        let Some(model) = get_model_by_id(model_id) else {
            return false;
        };

        model.model_type == expected_type && !model_is_non_llm(model_id)
    });
}

#[cfg(test)]
mod tests {
    use super::migrate_config;
    use crate::config::{Config, Preset, ProcessingBlock};

    #[test]
    fn migrate_config_rewrites_retired_model_ids() {
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
                model: "cerebras_zai_glm_4_7".to_string(),
                ..Default::default()
            }],
            ..Default::default()
        };

        let mut config = Config {
            presets: vec![builtin, custom],
            ..Default::default()
        };

        migrate_config(&mut config);

        assert_eq!(
            config.presets[0].blocks[0].model,
            crate::model_config::DEFAULT_IMAGE_MODEL_ID
        );
        assert_eq!(
            config.presets[1].blocks[0].model,
            crate::model_config::DEFAULT_CEREBRAS_TEXT_MODEL_ID
        );
    }

    #[test]
    fn migrate_config_updates_builtin_gemini_image_blocks_to_default() {
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

        let mut config = Config {
            presets: vec![builtin, custom],
            ..Default::default()
        };

        migrate_config(&mut config);

        assert_eq!(
            config.presets[0].blocks[0].model,
            crate::model_config::DEFAULT_IMAGE_MODEL_ID
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
            "cerebras_gpt_oss".to_string(),
            "qr-scanner".to_string(),
            "scout".to_string(),
            "text_accurate_kimi".to_string(),
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
                "cerebras_gpt_oss".to_string(),
                "text_accurate_kimi".to_string()
            ]
        );
    }
}

// ============================================================================
// CONFIG SAVING
// ============================================================================

/// Save config to disk
pub fn save_config(config: &Config) {
    let path = get_config_path();
    if let Ok(data) = serde_json::to_string_pretty(config) {
        let _ = std::fs::write(path, data);
    }
}

// ============================================================================
// LANGUAGE UTILITIES
// ============================================================================

lazy_static::lazy_static! {
    /// All available language names (sorted, deduplicated)
    static ref ALL_LANGUAGES: Vec<String> = {
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
    };
}

/// Get all available language names
pub fn get_all_languages() -> &'static Vec<String> {
    &ALL_LANGUAGES
}
