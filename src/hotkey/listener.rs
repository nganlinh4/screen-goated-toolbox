//! Messages to this process's hotkey listener, and when they are needed.

use super::{LISTENER_HWND, WM_RELOAD_HOTKEYS};
use crate::config::{Config, Hotkey};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_HOTKEY};

/// Posts to the listener this process created. A global window lookup could
/// reach another running instance (for example a dev and a release build).
pub(crate) fn post(message: u32, wparam: WPARAM, lparam: LPARAM) -> bool {
    let Ok(listener) = LISTENER_HWND.lock() else {
        return false;
    };
    !listener.0.is_invalid()
        && unsafe { PostMessageW(Some(listener.0), message, wparam, lparam) }.is_ok()
}

pub(crate) fn reload_registrations() {
    post(WM_RELOAD_HOTKEYS, WPARAM(0), LPARAM(0));
}

/// Fires a preset's first binding through the listener, as its hotkey would.
pub(crate) fn trigger_preset(preset_idx: usize) -> bool {
    let id = preset_hotkey_id(preset_idx, 0);
    post(WM_HOTKEY, WPARAM(id as usize), LPARAM(0))
}

/// Registration ID of a preset's hotkey; IDs below 1000 per preset are its bindings.
pub(crate) fn preset_hotkey_id(preset_idx: usize, hotkey_idx: usize) -> i32 {
    preset_idx as i32 * 1000 + hotkey_idx as i32 + 1
}

/// The preset a registration ID made by [`preset_hotkey_id`] belongs to.
pub(crate) fn preset_index(hotkey_id: i32) -> usize {
    ((hotkey_id - 1) / 1000) as usize
}

/// WM_ENDSESSION's wParam is nonzero only when Windows is really ending the
/// session; it may terminate the process any time after the message returns.
pub(super) fn session_is_ending(wparam: WPARAM) -> bool {
    wparam.0 != 0
}

/// Records a deliberate exit so a logoff or shutdown is not filed as a crash.
pub(super) fn exit_for_session_end() -> ! {
    crate::crash_report::exit(0, "session-end")
}

/// Every binding `register_all_hotkeys` reads. A save that leaves this
/// unchanged needs no unregister/register cycle.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct RegisteredBindings {
    presets: Vec<Vec<Hotkey>>,
    computer_control: Vec<Hotkey>,
    screen_translate: [Vec<Hotkey>; 3],
    live_translate: Vec<Hotkey>,
    screen_record: Vec<Hotkey>,
    translation_gummy: Vec<Hotkey>,
}

impl RegisteredBindings {
    pub(crate) fn of(config: &Config) -> Self {
        Self {
            presets: config
                .presets
                .iter()
                .map(|preset| preset.hotkeys.clone())
                .collect(),
            computer_control: config.computer_control_hotkeys.clone(),
            screen_translate: [
                config.screen_translate.hotkeys.clone(),
                config.screen_translate.fullscreen_hotkeys.clone(),
                config.screen_translate.subtitle_hotkeys.clone(),
            ],
            live_translate: config.live_translate.hotkeys.clone(),
            screen_record: config.screen_record_hotkeys.clone(),
            translation_gummy: config.translation_gummy.hotkeys.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{RegisteredBindings, preset_hotkey_id, preset_index, session_is_ending};
    use crate::config::Config;
    use windows::Win32::Foundation::WPARAM;

    #[test]
    fn preset_hotkey_ids_round_trip_to_their_preset() {
        for preset in [0, 1, 7, 2000] {
            for binding in [0, 1, 998] {
                assert_eq!(preset_index(preset_hotkey_id(preset, binding)), preset);
            }
        }
        assert_eq!(preset_hotkey_id(3, 0), 3001);
    }

    /// Only the listener's creator names its window class; every other caller
    /// posts to this process's own listener handle, never a global lookup.
    #[test]
    fn no_caller_finds_the_listener_by_a_global_window_lookup() {
        fn visit(dir: &std::path::Path, offenders: &mut Vec<String>) {
            for entry in std::fs::read_dir(dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    visit(&path, offenders);
                } else if path.extension().is_some_and(|ext| ext == "rs")
                    && !path.ends_with("hotkey/mod.rs")
                    && !path.ends_with("hotkey/listener.rs")
                    && std::fs::read_to_string(&path)
                        .is_ok_and(|source| source.contains("HotkeyListenerClass"))
                {
                    offenders.push(path.display().to_string());
                }
            }
        }
        let mut offenders = Vec::new();
        visit(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut offenders,
        );
        assert!(
            offenders.is_empty(),
            "global listener lookups: {offenders:?}"
        );
    }

    #[test]
    fn only_a_confirmed_session_end_exits() {
        assert!(session_is_ending(WPARAM(1)));
        assert!(!session_is_ending(WPARAM(0)));
    }

    #[test]
    fn only_binding_edits_change_the_registered_bindings() {
        let config = Config::default();
        let before = RegisteredBindings::of(&config);
        let mut edited = config.clone();
        edited.ui_language = "ko".to_string();
        assert_eq!(RegisteredBindings::of(&edited), before);
        edited
            .live_translate
            .hotkeys
            .push(crate::config::Hotkey::new(0x70, 0x0002));
        assert_ne!(RegisteredBindings::of(&edited), before);
    }
}
