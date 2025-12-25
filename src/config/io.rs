//! Config I/O operations: load, save, and language utilities.

use std::path::PathBuf;

use super::config_struct::Config;
use super::preset::Preset;
use super::types::ProcessingBlock;

pub fn get_config_path() -> PathBuf {
    let config_dir = dirs::config_dir()
        .unwrap_or_default()
        .join("screen-goated-toolbox");
    let _ = std::fs::create_dir_all(&config_dir);
    config_dir.join("config_v3.json")
}

/// Get all default preset IDs (those that start with "preset_")
/// These are the built-in presets that ship with the app
fn get_default_presets() -> Vec<Preset> {
    Config::default().presets
}

pub fn load_config() -> Config {
    let path = get_config_path();
    if path.exists() {
        let data = std::fs::read_to_string(path).unwrap_or_default();
        let mut config: Config = serde_json::from_str(&data).unwrap_or_default();
        
        // --- AUTO-MERGE NEW DEFAULT PRESETS ---
        // This ensures users get new presets from updates without losing their custom presets
        // or modifications to existing presets.
        //
        // Strategy:
        // 1. Default presets have IDs starting with "preset_" (e.g., "preset_translate")
        // 2. User-created presets have timestamp-based IDs (e.g., "1a2b3c4d5e")
        // 3. For each default preset:
        //    - If NOT in user's config → add it (new feature!)
        //    - If already in user's config → keep user's version (they may have customized it)
        
        let default_presets = get_default_presets();
        let existing_ids: std::collections::HashSet<String> = config.presets.iter()
            .map(|p| p.id.clone())
            .collect();
        
        // Find new default presets that don't exist in user's config
        let mut new_presets: Vec<Preset> = Vec::new();
        for default_preset in default_presets {
            // Only process built-in presets (those with "preset_" prefix)
            if default_preset.id.starts_with("preset_") && !existing_ids.contains(&default_preset.id) {
                new_presets.push(default_preset);
            }
        }
        
        // Append new presets to the end of user's preset list
        if !new_presets.is_empty() {
            config.presets.extend(new_presets);
        }
        
        // --- MIGRATE CRITICAL SETTINGS FOR EXISTING BUILT-IN PRESETS ---
        // When default presets are updated with new settings (like auto_paste=true),
        // we need to sync those settings to existing user presets.
        // This fixes the issue where auto_paste doesn't work initially for old configs.
        {
            let default_presets = get_default_presets();
            for preset in &mut config.presets {
                // Only update built-in presets (those with "preset_" prefix)
                if preset.id.starts_with("preset_") {
                    // Find the matching default preset
                    if let Some(default_preset) = default_presets.iter().find(|p| p.id == preset.id) {
                        // Sync auto_paste and auto_paste_newline from defaults
                        // This ensures new default settings are applied even to existing presets
                        preset.auto_paste = default_preset.auto_paste;
                        preset.auto_paste_newline = default_preset.auto_paste_newline;
                        
                        // Also sync auto_stop_recording for audio presets
                        if preset.preset_type == "audio" {
                            preset.auto_stop_recording = default_preset.auto_stop_recording;
                        }
                    }
                }
            }
        }
        
        // Safety check: Ensure every preset has at least one block matching its type
        for preset in &mut config.presets {
            // If empty, add default block based on preset type
            if preset.blocks.is_empty() {
                preset.blocks.push(ProcessingBlock {
                    block_type: preset.preset_type.clone(),
                    ..Default::default()
                });
            }
        }
        config
    } else {
        Config::default()
    }
}

pub fn save_config(config: &Config) {
    let path = get_config_path();
    let data = serde_json::to_string_pretty(config).unwrap();
    let _ = std::fs::write(path, data);
}

lazy_static::lazy_static! {
    static ref ALL_LANGUAGES: Vec<String> = {
        let mut languages = Vec::new();
        for i in 0..10000 {
            if let Some(lang) = isolang::Language::from_usize(i) {
                // Only include if it has an ISO 639-1 code (major languages)
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

pub fn get_all_languages() -> &'static Vec<String> {
    &ALL_LANGUAGES
}
