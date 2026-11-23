//! Host-owned recorder settings updates.

use sgt_recorder_protocol::{
    Command, RecorderHotkey, RecorderSettings, SettingsChange, SettingsEvent,
};
use std::collections::HashSet;
use std::sync::{LazyLock, Mutex};

static ACTIVE_SESSIONS: LazyLock<Mutex<HashSet<String>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));
/// The worker session whose binding capture has paused the host's hotkeys.
static CAPTURING_SESSION: Mutex<Option<String>> = Mutex::new(None);

pub(super) fn register_session(token: &str) {
    ACTIVE_SESSIONS.lock().unwrap().insert(token.to_owned());
}

pub(super) fn retire_session(token: &str) {
    ACTIVE_SESSIONS.lock().unwrap().remove(token);
    // A worker that exits mid-capture must not leave every hotkey paused.
    if capture_transition(&CAPTURING_SESSION, token, false) {
        crate::hotkey::set_binding_capture(crate::hotkey::CaptureOwner::Recorder, false);
    }
}

/// Records which session holds the recorder's capture. Returns whether the
/// host's recorder claim must be set to `capturing`; a session that does not
/// hold the capture cannot end another session's.
fn capture_transition(holder: &Mutex<Option<String>>, token: &str, capturing: bool) -> bool {
    let mut holder = holder.lock().unwrap_or_else(|error| error.into_inner());
    if capturing {
        *holder = Some(token.to_owned());
        true
    } else if holder.as_deref() == Some(token) {
        *holder = None;
        true
    } else {
        false
    }
}

fn with_active_session<T>(token: &str, action: impl FnOnce() -> T) -> Option<T> {
    let active = ACTIVE_SESSIONS.lock().unwrap();
    if !active.contains(token) {
        return None;
    }
    let result = action();
    drop(active);
    Some(result)
}

fn snapshot(config: &crate::config::Config) -> RecorderSettings {
    RecorderSettings {
        hotkeys: config
            .screen_record_hotkeys
            .iter()
            .map(|h| RecorderHotkey {
                code: h.code,
                modifiers: h.modifiers,
            })
            .collect(),
        project_limit: config.max_screen_record_projects,
        upload_limit: config.max_screen_record_recent_uploads,
        window_size: config.screen_record_window_size,
    }
}

fn apply(
    config: &mut crate::config::Config,
    change: &SettingsChange,
    save: impl FnOnce(&crate::config::Config) -> std::io::Result<()>,
) -> Result<RecorderSettings, String> {
    let mut next = config.clone();
    let changed = match change {
        SettingsChange::Snapshot | SettingsChange::BindingCapture(_) => false,
        SettingsChange::ProjectLimit(limit) if (10..=100).contains(limit) => {
            next.max_screen_record_projects = *limit;
            true
        }
        SettingsChange::UploadLimit(limit) if (4..=24).contains(limit) => {
            next.max_screen_record_recent_uploads = *limit;
            true
        }
        SettingsChange::WindowSize((width, height)) if *width >= 800 && *height >= 500 => {
            next.screen_record_window_size = (*width, *height);
            true
        }
        SettingsChange::AddHotkey(binding) => {
            let valid_modifiers = crate::hotkey::MOD_CONTROL
                | crate::hotkey::MOD_ALT
                | crate::hotkey::MOD_SHIFT
                | crate::hotkey::MOD_WIN;
            if binding.code == 0 || binding.code > 255 || binding.modifiers & !valid_modifiers != 0
            {
                return Err("Invalid recorder hotkey".to_string());
            }
            if next
                .screen_record_hotkeys
                .iter()
                .any(|h| h.code == binding.code && h.modifiers == binding.modifiers)
            {
                false
            } else {
                if let Some(conflict) =
                    next.check_hotkey_conflict(binding.code, binding.modifiers, None)
                {
                    let text = crate::gui::locale::LocaleText::get(&next.ui_language);
                    return Err(text.hotkey_conflict_message(&conflict));
                }
                next.screen_record_hotkeys
                    .push(crate::config::Hotkey::new(binding.code, binding.modifiers));
                true
            }
        }
        SettingsChange::RemoveHotkey(binding) => {
            if let Some(index) = next
                .screen_record_hotkeys
                .iter()
                .position(|h| h.code == binding.code && h.modifiers == binding.modifiers)
            {
                next.screen_record_hotkeys.remove(index);
                true
            } else {
                false
            }
        }
        _ => return Err("Invalid recorder settings value".to_string()),
    };
    if changed {
        save(&next).map_err(|error| format!("Could not save recorder settings: {error}"))?;
        *config = next;
    }
    Ok(snapshot(config))
}

pub(super) fn process(event: SettingsEvent) -> Option<Command> {
    let token = event.token.clone();
    with_active_session(&token, || process_active(event))
}

fn process_active(event: SettingsEvent) -> Command {
    let result = {
        let mut app = crate::APP.lock().unwrap();
        apply(
            &mut app.config,
            &event.change,
            crate::config::try_save_config,
        )
    };
    if result.is_ok() {
        match event.change {
            SettingsChange::AddHotkey(_) | SettingsChange::RemoveHotkey(_) => {
                crate::hotkey::listener::reload_registrations();
            }
            SettingsChange::BindingCapture(capturing)
                if capture_transition(&CAPTURING_SESSION, &event.token, capturing) =>
            {
                crate::hotkey::set_binding_capture(
                    crate::hotkey::CaptureOwner::Recorder,
                    capturing,
                );
            }
            _ => {}
        }
    }
    let (settings, error) = match result {
        Ok(settings) => (Some(settings), None),
        Err(error) => {
            crate::log_info!("[ScreenRecord] settings update failed: {error}");
            (None, Some(error.chars().take(1000).collect()))
        }
    };
    Command::SettingsAck {
        event_id: event.event_id,
        settings,
        error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binding_capture_is_saved_nowhere_and_ends_only_with_its_session() {
        let mut config = crate::config::Config::default();
        let settings = apply(&mut config, &SettingsChange::BindingCapture(true), |_| {
            panic!("binding capture must not save settings")
        })
        .unwrap();
        assert_eq!(settings, snapshot(&config));

        let holder = Mutex::new(None);
        let old = "01".repeat(sgt_recorder_protocol::TOKEN_BYTES);
        let new = "02".repeat(sgt_recorder_protocol::TOKEN_BYTES);
        assert!(capture_transition(&holder, &old, true));
        assert!(capture_transition(&holder, &new, true));
        // The retired session's late release cannot resume the new capture.
        assert!(!capture_transition(&holder, &old, false));
        assert!(capture_transition(&holder, &new, false));
        assert!(!capture_transition(&holder, &new, false));
    }
    #[test]
    fn buffered_event_from_retired_worker_cannot_mutate_a_new_session() {
        let old = "01".repeat(sgt_recorder_protocol::TOKEN_BYTES);
        let new = "02".repeat(sgt_recorder_protocol::TOKEN_BYTES);
        register_session(&old);
        assert_eq!(with_active_session(&old, || 1), Some(1));
        retire_session(&old);
        register_session(&new);
        assert_eq!(
            with_active_session(&old, || panic!("retired worker event ran")),
            None::<()>
        );
        assert_eq!(with_active_session(&new, || 2), Some(2));
        retire_session(&new);
    }
    #[test]
    fn updates_only_owned_fields_and_preserves_unrelated_changes() {
        let mut config = crate::config::Config::default();
        config.tts_playground.draft_text = "recent edit".into();
        let settings = apply(&mut config, &SettingsChange::ProjectLimit(72), |_| Ok(())).unwrap();
        assert_eq!(settings.project_limit, 72);
        assert_eq!(config.tts_playground.draft_text, "recent edit");
    }
    #[test]
    fn failed_persistence_leaves_memory_unchanged() {
        let mut config = crate::config::Config::default();
        let before = config.max_screen_record_projects;
        assert!(
            apply(&mut config, &SettingsChange::ProjectLimit(72), |_| Err(
                std::io::Error::other("disk full")
            ))
            .is_err()
        );
        assert_eq!(config.max_screen_record_projects, before);
    }
    #[test]
    fn repeated_requests_after_lost_ack_are_idempotent() {
        let mut config = crate::config::Config::default();
        let binding = RecorderHotkey {
            code: 0x70,
            modifiers: 0,
        };
        apply(
            &mut config,
            &SettingsChange::AddHotkey(binding.clone()),
            |_| Ok(()),
        )
        .unwrap();
        apply(
            &mut config,
            &SettingsChange::AddHotkey(binding.clone()),
            |_| panic!("duplicate should not save"),
        )
        .unwrap();
        apply(
            &mut config,
            &SettingsChange::RemoveHotkey(binding.clone()),
            |_| Ok(()),
        )
        .unwrap();
        apply(&mut config, &SettingsChange::RemoveHotkey(binding), |_| {
            panic!("missing binding should not save")
        })
        .unwrap();
    }

    #[test]
    fn removal_targets_the_displayed_binding_after_a_concurrent_reorder() {
        let mut config = crate::config::Config::default();
        let originally_first = RecorderHotkey {
            code: 0x70,
            modifiers: 0,
        };
        let other = RecorderHotkey {
            code: 0x71,
            modifiers: 0,
        };
        config.screen_record_hotkeys = vec![
            crate::config::Hotkey::new(other.code, other.modifiers),
            crate::config::Hotkey::new(originally_first.code, originally_first.modifiers),
        ];
        let result = apply(
            &mut config,
            &SettingsChange::RemoveHotkey(originally_first),
            |_| Ok(()),
        )
        .unwrap();
        assert_eq!(result.hotkeys, vec![other]);
    }
}
