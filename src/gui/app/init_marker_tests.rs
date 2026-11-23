use super::*;

#[test]
fn marker_commit_uses_current_config_and_preserves_unrelated_edit() {
    let mut current = Config::default();
    current
        .mark_staged_preset_model_update("1.0.0".to_string())
        .unwrap();
    // Simulate an edit after the earlier SettingsApp snapshot was taken.
    current.api_key = "newer local edit".to_string();
    let mut saved = None;
    assert!(!prepare_startup_model_update(
        &mut current,
        "1.0.0",
        |config| {
            saved = Some(config.clone());
            Ok(())
        }
    ));
    let saved = saved.unwrap();
    assert_eq!(saved.api_key, "newer local edit");
    assert!(saved.pending_preset_model_update.is_none());
    assert_eq!(current.api_key, saved.api_key);
    assert!(current.pending_preset_model_update.is_none());
}

#[test]
fn failed_marker_save_keeps_live_marker_for_retry() {
    let mut current = Config::default();
    current
        .mark_staged_preset_model_update("1.0.0".to_string())
        .unwrap();
    assert!(!prepare_startup_model_update(&mut current, "1.0.0", |_| {
        Err(std::io::Error::other("unavailable"))
    }));
    assert!(current.pending_preset_model_update.is_some());
}

#[test]
fn later_global_draft_edit_survives_startup_local_settings_changes() {
    let mut current = Config::default();
    current.tts_playground.draft_text = "original".into();
    current
        .mark_staged_preset_model_update("1.0.0".into())
        .unwrap();
    let (mut edited, baseline, _) = capture_startup_snapshot(&mut current, "1.0.0", |_| Ok(()));

    // A different window changes the draft during the remaining startup work,
    // while startup itself derives a setting from the original snapshot.
    current.tts_playground.draft_text = "new draft".into();
    edited.authorized_startup_path = "startup-path".into();
    let merged = super::super::config_merge::merge_config(&baseline, &edited, &current).unwrap();
    assert_eq!(merged.tts_playground.draft_text, "new draft");
    assert_eq!(merged.authorized_startup_path, "startup-path");
    assert!(merged.pending_preset_model_update.is_none());
}
