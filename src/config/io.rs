//! Config I/O operations: load, save, and language utilities.

use std::path::PathBuf;
#[cfg(not(feature = "recorder-worker"))]
use std::sync::Mutex;

use crate::config::Config;
use crate::config::preset::{Preset, ProcessingBlock, get_default_presets};

#[path = "io_read_guard.rs"]
mod read_guard;
#[path = "io_secrets.rs"]
mod secrets;
#[cfg(not(feature = "recorder-worker"))]
pub(crate) use read_guard::unread_config_path;

const RETIRED_BUILTIN_PRESET_MIGRATIONS: &[(&str, &str)] = &[
    ("preset_extract_retranslate", "preset_translate"),
    (
        "preset_extract_retrans_retrans",
        "preset_translate_retranslate",
    ),
];

// ============================================================================
// CONFIG PATH
// ============================================================================

/// Main configuration path. An isolated runtime starts from defaults when its
/// own file does not exist; it never falls back to the user's configuration.
pub fn get_config_path() -> PathBuf {
    let config_dir = crate::paths::app_runtime_config_dir();
    let _ = std::fs::create_dir_all(&config_dir);
    config_dir.join("config_v3.json")
}

// ============================================================================
// CONFIG LOADING
// ============================================================================

/// Load config from disk, merging with defaults as needed
pub fn load_config() -> Config {
    let LoadedConfig {
        config,
        disk_unread,
        persist_migration,
        unreadable_keys,
    } = load_config_with(
        &get_config_path(),
        &mut |path| std::fs::read(path),
        read_guard::READ_RETRY_DELAY,
    );
    #[cfg(not(feature = "recorder-worker"))]
    {
        read_guard::mark_disk_config_unread(disk_unread);
        secrets::keep_unreadable_for_saves(unreadable_keys);
        if persist_migration {
            save_config(&config);
        }
    }
    // The recorder worker never saves the config.
    #[cfg(feature = "recorder-worker")]
    let _ = (disk_unread, persist_migration, unreadable_keys);
    config
}

struct LoadedConfig {
    config: Config,
    /// An existing file was neither loaded nor preserved; saving would lose it.
    disk_unread: bool,
    /// Loading migrated data that only a save makes permanent (retired
    /// built-in presets, or API keys still stored as legacy plaintext).
    persist_migration: bool,
    /// Stored API keys that could not be unsealed; saves write them back.
    unreadable_keys: secrets::UnreadableKeys,
}

impl LoadedConfig {
    fn defaults(disk_unread: bool) -> Self {
        Self {
            config: Config::default(),
            disk_unread,
            persist_migration: false,
            unreadable_keys: secrets::UnreadableKeys::default(),
        }
    }
}

fn load_config_with(
    path: &std::path::Path,
    read: &mut impl FnMut(&std::path::Path) -> std::io::Result<Vec<u8>>,
    retry_delay: std::time::Duration,
) -> LoadedConfig {
    // Every failure to decode an existing file (I/O, non-UTF-8 text, or a hard
    // parse error; migrate_config tolerates field drift via #[serde(default)])
    // preserves the file before falling back to defaults, because the next save
    // replaces it and would otherwise discard every preset, profile and API key.
    let data = match read_guard::read_with_retry(path, read, retry_delay) {
        Ok(d) => d,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return LoadedConfig::defaults(false);
        }
        Err(e) => {
            // Still locked or unreadable: a copy usually fails the same way, so
            // keep saves away from the file until a later launch can load it.
            backup_corrupt_config(path, None, &e);
            return LoadedConfig::defaults(true);
        }
    };

    let mut config: Config = match serde_json::from_slice(&data) {
        Ok(c) => c,
        Err(e) => {
            let preserved = backup_corrupt_config(path, Some(&data), &e);
            return LoadedConfig::defaults(!preserved);
        }
    };

    let unsealed = secrets::unseal_api_keys(&mut config);

    #[cfg(feature = "recorder-worker")]
    let persist_retirement = false;
    #[cfg(not(feature = "recorder-worker"))]
    let persist_retirement = config
        .presets
        .iter()
        .any(|preset| is_retired_builtin_preset_id(&preset.id))
        || config.preset_profiles.iter().any(|profile| {
            profile
                .presets
                .iter()
                .any(|preset| is_retired_builtin_preset_id(&preset.id))
        });

    // Apply migrations and merge new defaults
    migrate_config(&mut config);
    LoadedConfig {
        config,
        disk_unread: false,
        persist_migration: persist_retirement || unsealed.legacy_plaintext,
        unreadable_keys: unsealed.unreadable,
    }
}

/// Preserve a config file that could not be loaded in a timestamped
/// `.corrupt-*` sibling (from the bytes already read when there are any), so
/// the user's data can be recovered. Returns whether the copy exists.
fn backup_corrupt_config(
    path: &std::path::Path,
    contents: Option<&[u8]>,
    err: &dyn std::fmt::Display,
) -> bool {
    let ts = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let mut backup = path.as_os_str().to_owned();
    backup.push(format!(".corrupt-{ts}"));
    let backup = PathBuf::from(backup);
    let copied = match contents {
        Some(bytes) => std::fs::write(&backup, bytes).is_ok(),
        None => std::fs::copy(path, &backup).is_ok(),
    };
    crate::log_info!(
        "[config] config could not be loaded ({err}); {} -> {}",
        if copied {
            "preserved it at"
        } else {
            "FAILED to back it up to"
        },
        backup.display()
    );
    copied
}

/// Apply config migrations and merge new default presets
fn migrate_config(config: &mut Config) {
    let default_presets = get_default_presets();

    migrate_computer_control_launcher(config);
    migrate_live_translate_launcher(config);
    config.ensure_preset_profiles();
    let offered: std::collections::HashSet<String> =
        config.offered_builtin_preset_ids.iter().cloned().collect();
    migrate_preset_list(&mut config.presets, &default_presets, &offered);
    migrate_retired_builtin_presets(&mut config.presets, &mut config.active_preset_idx);
    for profile in &mut config.preset_profiles {
        migrate_preset_list(&mut profile.presets, &default_presets, &offered);
        migrate_retired_builtin_presets(&mut profile.presets, &mut profile.active_preset_idx);
        profile.active_preset_idx = profile
            .active_preset_idx
            .min(profile.presets.len().saturating_sub(1));
    }
    record_offered_builtin_presets(config, &default_presets);

    config
        .adaptive_model_priority
        .image_to_text_overrides
        .normalize();
    config
        .adaptive_model_priority
        .text_to_text_overrides
        .normalize();

    #[cfg(not(feature = "recorder-worker"))]
    {
        use crate::retry_model_chain::normalized_chosen_after;
        let chains = &mut config.model_priority_chains;
        chains.image_chosen_after =
            normalized_chosen_after(&chains.image_to_text, chains.image_chosen_after);
        chains.text_chosen_after =
            normalized_chosen_after(&chains.text_to_text, chains.text_chosen_after);
    }
    normalize_model_priority_chain(&mut config.model_priority_chains.image_to_text);
    normalize_model_priority_chain(&mut config.model_priority_chains.text_to_text);
    normalize_translation_gummy_settings(config);
    config.max_history_items = config.max_history_items.clamp(
        crate::config::types::MIN_HISTORY_LIMIT,
        crate::config::types::MAX_HISTORY_LIMIT,
    );
    config.screen_translate = config.screen_translate.clone().normalized();

    if config.realtime_translation_model == "taalas-rt" {
        config.realtime_translation_model =
            crate::model_config::REALTIME_TRANSLATION_MODEL_LLM.to_string();
    }

    config.sync_active_profile_from_presets();
}

/// Live Translate used to masquerade as a realtime-audio preset. Promote its
/// launcher-owned state and remove the legacy row from every preset profile.
fn migrate_live_translate_launcher(config: &mut Config) {
    const LEGACY_ID: &str = "preset_realtime_audio_translate";

    let mut migrated_hotkeys = std::mem::take(&mut config.live_translate.hotkeys);
    remove_live_translate_launcher(
        &mut config.presets,
        &mut config.active_preset_idx,
        LEGACY_ID,
        &mut migrated_hotkeys,
    );
    for profile in &mut config.preset_profiles {
        remove_live_translate_launcher(
            &mut profile.presets,
            &mut profile.active_preset_idx,
            LEGACY_ID,
            &mut migrated_hotkeys,
        );
    }
    config.live_translate.hotkeys = migrated_hotkeys;
}

fn remove_live_translate_launcher(
    presets: &mut Vec<Preset>,
    active_idx: &mut usize,
    legacy_id: &str,
    migrated_hotkeys: &mut Vec<crate::config::Hotkey>,
) {
    while let Some(idx) = presets.iter().position(|preset| preset.id == legacy_id) {
        let legacy = presets.remove(idx);
        for hotkey in legacy.hotkeys {
            if !migrated_hotkeys.iter().any(|existing| {
                existing.code == hotkey.code && existing.modifiers == hotkey.modifiers
            }) {
                migrated_hotkeys.push(hotkey);
            }
        }
        if *active_idx > idx {
            *active_idx -= 1;
        }
        *active_idx = (*active_idx).min(presets.len().saturating_sub(1));
    }
}

/// Computer Control used to masquerade as a realtime-audio preset. Move its
/// hotkeys into global app state and remove the legacy row from every profile.
fn migrate_computer_control_launcher(config: &mut Config) {
    const LEGACY_ID: &str = "preset_computer_control";

    let mut migrated_hotkeys = std::mem::take(&mut config.computer_control_hotkeys);
    remove_legacy_launcher(
        &mut config.presets,
        &mut config.active_preset_idx,
        LEGACY_ID,
        &mut migrated_hotkeys,
    );
    for profile in &mut config.preset_profiles {
        remove_legacy_launcher(
            &mut profile.presets,
            &mut profile.active_preset_idx,
            LEGACY_ID,
            &mut migrated_hotkeys,
        );
    }
    config.computer_control_hotkeys = migrated_hotkeys;
}

fn remove_legacy_launcher(
    presets: &mut Vec<Preset>,
    active_idx: &mut usize,
    legacy_id: &str,
    migrated_hotkeys: &mut Vec<crate::config::Hotkey>,
) {
    while let Some(idx) = presets.iter().position(|preset| preset.id == legacy_id) {
        let legacy = presets.remove(idx);
        for hotkey in legacy.hotkeys {
            if !migrated_hotkeys.iter().any(|existing| {
                existing.code == hotkey.code && existing.modifiers == hotkey.modifiers
            }) {
                migrated_hotkeys.push(hotkey);
            }
        }

        if *active_idx > idx {
            *active_idx -= 1;
        }
        *active_idx = (*active_idx).min(presets.len().saturating_sub(1));
    }
}

#[cfg(not(feature = "recorder-worker"))]
fn is_retired_builtin_preset_id(preset_id: &str) -> bool {
    RETIRED_BUILTIN_PRESET_MIGRATIONS
        .iter()
        .any(|(retired_id, _)| *retired_id == preset_id)
}

fn migrate_retired_builtin_presets(presets: &mut Vec<Preset>, active_idx: &mut usize) {
    for &(retired_id, replacement_id) in RETIRED_BUILTIN_PRESET_MIGRATIONS {
        migrate_retired_builtin_preset(presets, active_idx, retired_id, replacement_id);
    }
}

fn migrate_retired_builtin_preset(
    presets: &mut Vec<Preset>,
    active_idx: &mut usize,
    retired_id: &str,
    replacement_id: &str,
) {
    while let Some(retired_idx) = presets.iter().position(|preset| preset.id == retired_id) {
        let was_active = *active_idx == retired_idx;
        let retired = presets.remove(retired_idx);

        if *active_idx > retired_idx {
            *active_idx -= 1;
        }

        if let Some(replacement_idx) = presets
            .iter()
            .position(|preset| preset.id == replacement_id)
        {
            let replacement = &mut presets[replacement_idx];
            replacement.is_favorite |= retired.is_favorite;
            for hotkey in retired.hotkeys {
                if !replacement.hotkeys.iter().any(|existing| {
                    existing.code == hotkey.code && existing.modifiers == hotkey.modifiers
                }) {
                    replacement.hotkeys.push(hotkey);
                }
            }
            if was_active {
                *active_idx = replacement_idx;
            }
        }

        *active_idx = (*active_idx).min(presets.len().saturating_sub(1));
    }
}

/// Appends built-ins that are new since the last startup. A built-in already
/// offered and absent from a list was deleted by the user and stays deleted.
fn migrate_preset_list(
    presets: &mut Vec<Preset>,
    default_presets: &[Preset],
    offered: &std::collections::HashSet<String>,
) {
    let existing_ids: std::collections::HashSet<String> =
        presets.iter().map(|p| p.id.clone()).collect();

    let new_presets: Vec<Preset> = default_presets
        .iter()
        .filter(|p| p.is_builtin() && !existing_ids.contains(&p.id) && !offered.contains(&p.id))
        .cloned()
        .collect();

    if !new_presets.is_empty() {
        presets.extend(new_presets);
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

fn record_offered_builtin_presets(config: &mut Config, default_presets: &[Preset]) {
    let offered = &mut config.offered_builtin_preset_ids;
    for preset in default_presets.iter().filter(|preset| preset.is_builtin()) {
        if !offered.contains(&preset.id) {
            offered.push(preset.id.clone());
        }
    }
}

fn normalize_model_priority_chain(chain: &mut Vec<String>) {
    let mut seen = std::collections::HashSet::new();
    chain.retain(|model_id| !model_id.trim().is_empty() && seen.insert(model_id.clone()));
}

fn normalize_translation_gummy_settings(config: &mut Config) {
    let defaults = crate::config::TranslationGummySettings::default();
    if config.translation_gummy.first.language.trim().is_empty() {
        config.translation_gummy.first = defaults.first;
    }
    if config.translation_gummy.second.language.trim().is_empty() {
        config.translation_gummy.second = defaults.second;
    }
    config.translation_gummy = config.translation_gummy.normalized();
}

// ============================================================================
// CONFIG SAVING
// ============================================================================

/// Save config to disk atomically (temp + rename), so an interrupted write
/// never truncates the single file that holds every preset, profile and API key.
#[cfg(not(feature = "recorder-worker"))]
pub fn save_config(config: &Config) {
    if let Err(e) = try_save_config(config) {
        crate::log_info!("[config] failed to save config: {e}");
    }
}

#[cfg(not(feature = "recorder-worker"))]
pub fn try_save_config(config: &Config) -> std::io::Result<()> {
    let _guard = CONFIG_SAVE_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    write_config(config)
}

#[cfg(not(feature = "recorder-worker"))]
static CONFIG_SAVE_LOCK: Mutex<()> = Mutex::new(());

/// Capture the current shared configuration in the same order as other
/// writers, but release the application lock before serializing and syncing.
#[cfg(not(feature = "recorder-worker"))]
pub(crate) fn try_save_current_config() -> std::io::Result<()> {
    save_current_with(
        &crate::APP,
        &CONFIG_SAVE_LOCK,
        |app| app.config.clone(),
        write_config,
    )
}

#[cfg(not(feature = "recorder-worker"))]
fn save_current_with<T, S>(
    state: &Mutex<T>,
    gate: &Mutex<()>,
    snapshot: impl FnOnce(&T) -> S,
    write: impl FnOnce(&S) -> std::io::Result<()>,
) -> std::io::Result<()> {
    let state = state
        .lock()
        .map_err(|_| std::io::Error::other("application state lock is poisoned"))?;
    let _guard = gate.lock().unwrap_or_else(|error| error.into_inner());
    let snapshot = snapshot(&state);
    drop(state);
    write(&snapshot)
}

#[cfg(not(feature = "recorder-worker"))]
fn write_config(config: &Config) -> std::io::Result<()> {
    read_guard::refuse_if_disk_config_unread()?;
    let path = get_config_path();
    let mut config_to_save = secrets::sealed_for_save(config)?;
    config_to_save.sync_active_profile_from_presets();
    crate::atomic_json::write_json_atomic(&path, &config_to_save)
}

#[cfg(all(test, not(feature = "recorder-worker")))]
#[path = "io_tests.rs"]
mod tests;

#[cfg(all(test, not(feature = "recorder-worker")))]
#[path = "io_save_tests.rs"]
mod save_tests;

#[cfg(all(test, not(feature = "recorder-worker")))]
#[path = "io_unreadable_tests.rs"]
mod unreadable_tests;

#[cfg(all(test, not(feature = "recorder-worker")))]
#[path = "io_history_limit_tests.rs"]
mod history_limit_tests;

#[cfg(test)]
#[path = "io_isolation_tests.rs"]
mod isolation_tests;

#[cfg(test)]
#[path = "io_saved_model_tests.rs"]
mod saved_model_tests;

#[cfg(all(test, not(feature = "recorder-worker")))]
#[path = "io_live_translate_tests.rs"]
mod live_translate_tests;

#[cfg(all(test, not(feature = "recorder-worker")))]
#[path = "io_priority_preservation_tests.rs"]
mod priority_limit_tests;

#[cfg(all(test, not(feature = "recorder-worker")))]
#[path = "io_tts_migration_tests.rs"]
mod tts_migration_tests;

#[cfg(all(test, not(feature = "recorder-worker")))]
#[path = "io_gummy_migration_tests.rs"]
mod gummy_migration_tests;

#[cfg(all(test, not(feature = "recorder-worker")))]
#[path = "io_builtin_deletion_tests.rs"]
mod builtin_deletion_tests;
