//! Provider API keys are DPAPI-sealed on disk and plaintext in memory; legacy
//! plaintext still loads, and a value that cannot be unsealed clears only
//! that key.

use super::{SEALED_PREFIX, UnreadableKeys, sealed_for_disk};
use crate::config::Config;
use crate::config::io::{LoadedConfig, load_config_with};
use std::path::Path;
use std::time::Duration;

fn with_keys() -> Config {
    Config {
        api_key: "gsk_groq_secret".to_string(),
        gemini_api_key: "AIza-gemini-secret".to_string(),
        openrouter_api_key: "sk-or-secret".to_string(),
        nvidia_api_key: "nvapi-secret".to_string(),
        ui_language: "ko".to_string(),
        ..Config::default()
    }
}

/// Seal with nothing set aside from a failed unseal.
fn seal_all(config: &Config) -> Config {
    sealed_for_disk(config, &mut UnreadableKeys::default()).unwrap()
}

/// Drive the production loader over exactly these on-disk bytes.
fn load_bytes(bytes: Vec<u8>) -> LoadedConfig {
    load_config_with(
        Path::new("config_v3.json"),
        &mut |_| Ok(bytes.clone()),
        Duration::ZERO,
    )
}

fn keys(config: &Config) -> [&str; 4] {
    [
        &config.api_key,
        &config.gemini_api_key,
        &config.openrouter_api_key,
        &config.nvidia_api_key,
    ]
}

#[test]
fn sealed_keys_round_trip_and_never_reach_disk_in_plaintext() {
    let config = with_keys();
    let sealed = seal_all(&config);
    for value in keys(&sealed) {
        assert!(value.starts_with(SEALED_PREFIX), "{value}");
    }
    let bytes = serde_json::to_vec(&sealed).unwrap();
    let text = String::from_utf8(bytes.clone()).unwrap();
    for secret in keys(&config) {
        assert!(!text.contains(secret), "plaintext {secret} written to disk");
    }

    let loaded = load_bytes(bytes);
    assert_eq!(keys(&loaded.config), keys(&config));
    assert_eq!(loaded.config.ui_language, "ko");
    assert!(!loaded.persist_migration, "sealed keys need no re-save");
    // Sealing does not change the caller's in-memory config.
    assert_eq!(config.api_key, "gsk_groq_secret");
}

#[test]
fn legacy_plaintext_keys_load_and_request_a_sealed_resave() {
    let bytes = serde_json::to_vec(&with_keys()).unwrap();
    let loaded = load_bytes(bytes);
    assert_eq!(keys(&loaded.config), keys(&with_keys()));
    assert!(
        loaded.persist_migration,
        "plaintext keys must be re-saved sealed"
    );
}

#[test]
fn empty_keys_stay_empty_on_disk_and_after_loading() {
    let config = Config::default();
    let sealed = seal_all(&config);
    assert_eq!(keys(&sealed), ["", "", "", ""]);

    let loaded = load_bytes(serde_json::to_vec(&sealed).unwrap());
    assert_eq!(keys(&loaded.config), ["", "", "", ""]);
    assert!(!loaded.persist_migration);
}

#[test]
fn undecryptable_keys_load_empty_and_keep_every_other_setting() {
    let mut on_disk = seal_all(&with_keys());
    // Not base64 at all.
    on_disk.api_key = format!("{SEALED_PREFIX}%%% not base64 %%%");
    // Well-formed base64 that is not a DPAPI blob for this user, as when a
    // profile is copied to another machine.
    on_disk.gemini_api_key = format!("{SEALED_PREFIX}AAECAwQFBgcICQoLDA0ODw==");
    // A blob sealed for this user but truncated in transit.
    let truncated = &on_disk.nvidia_api_key[..on_disk.nvidia_api_key.len() / 2];
    on_disk.nvidia_api_key = truncated.to_string();

    let loaded = load_bytes(serde_json::to_vec(&on_disk).unwrap());

    assert_eq!(loaded.config.api_key, "");
    assert_eq!(loaded.config.gemini_api_key, "");
    assert_eq!(loaded.config.nvidia_api_key, "");
    assert_eq!(loaded.config.openrouter_api_key, "sk-or-secret");
    assert_eq!(loaded.config.ui_language, "ko");
    assert!(!loaded.disk_unread, "the file itself loaded");
    assert!(!loaded.config.presets.is_empty(), "presets survive");
}

#[test]
fn a_save_after_a_failed_unseal_keeps_the_stored_value() {
    let mut on_disk = seal_all(&with_keys());
    // Sealed for another Windows user or machine: unreadable here, but still
    // the user's real key wherever it was sealed.
    let foreign = format!("{SEALED_PREFIX}AAECAwQFBgcICQoLDA0ODw==");
    on_disk.gemini_api_key = foreign.clone();

    let mut loaded = load_bytes(serde_json::to_vec(&on_disk).unwrap());
    assert_eq!(loaded.config.gemini_api_key, "");

    // Any later save (a settings change, the migration re-save) must not
    // replace the stored value with an empty key.
    let resaved = sealed_for_disk(&loaded.config, &mut loaded.unreadable_keys).unwrap();
    assert_eq!(resaved.gemini_api_key, foreign);
    assert!(resaved.api_key.starts_with(SEALED_PREFIX));

    // A key the user enters replaces the stored value for good, so clearing
    // it afterwards saves an empty key.
    loaded.config.gemini_api_key = "AIza-replacement".to_string();
    let replaced = sealed_for_disk(&loaded.config, &mut loaded.unreadable_keys).unwrap();
    assert!(replaced.gemini_api_key.starts_with(SEALED_PREFIX));
    assert_ne!(replaced.gemini_api_key, foreign);
    loaded.config.gemini_api_key.clear();
    let cleared = sealed_for_disk(&loaded.config, &mut loaded.unreadable_keys).unwrap();
    assert_eq!(cleared.gemini_api_key, "");
}
