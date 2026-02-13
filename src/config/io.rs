//! Config I/O operations: load, save, and language utilities.

use std::path::PathBuf;

use crate::config::config::Config;
use crate::config::preset::{get_default_presets, Preset, ProcessingBlock};

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

            // Sync blocks (prompts, models, render modes) from defaults
            // This ensures built-in preset prompts stay up-to-date
            preset.blocks = default_preset.blocks.clone();

            // Sync audio-specific settings
            if preset.preset_type == "audio" {
                preset.auto_stop_recording = default_preset.auto_stop_recording;
            }
        }
    }

    // -------------------------------------------------------------------------
    // 3. ENSURE EVERY PRESET HAS AT LEAST ONE BLOCK
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
