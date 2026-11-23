//! Global hotkeys pause while a settings surface records a new binding.
//!
//! The settings window and the screen recorder record bindings independently.
//! Each owns its own claim, so one ending its capture (the settings window
//! reports its state every frame) never resumes hotkeys the other still holds.

use super::{WM_REGISTER_HOTKEYS, WM_UNREGISTER_HOTKEYS};
use std::sync::atomic::{AtomicU8, Ordering};
use windows::Win32::Foundation::{LPARAM, WPARAM};

#[derive(Clone, Copy, Debug)]
pub(crate) enum CaptureOwner {
    SettingsWindow = 1,
    Recorder = 2,
}

static OWNERS: AtomicU8 = AtomicU8::new(0);

/// Whether any owner is recording a binding right now.
pub(crate) fn active() -> bool {
    OWNERS.load(Ordering::SeqCst) != 0
}

pub(crate) fn set_binding_capture(owner: CaptureOwner, capturing: bool) {
    if let Some(paused) = update(&OWNERS, owner, capturing) {
        let message = if paused {
            WM_UNREGISTER_HOTKEYS
        } else {
            WM_REGISTER_HOTKEYS
        };
        super::listener::post(message, WPARAM(0), LPARAM(0));
    }
}

/// Applies one owner's state. Returns `Some(paused)` only when hotkeys as a
/// whole change between paused and live.
fn update(owners: &AtomicU8, owner: CaptureOwner, capturing: bool) -> Option<bool> {
    let bit = owner as u8;
    let previous = if capturing {
        owners.fetch_or(bit, Ordering::SeqCst)
    } else {
        owners.fetch_and(!bit, Ordering::SeqCst)
    };
    let next = if capturing {
        previous | bit
    } else {
        previous & !bit
    };
    ((previous != 0) != (next != 0)).then_some(next != 0)
}

#[cfg(test)]
mod tests {
    use super::{CaptureOwner, update};
    use std::sync::atomic::AtomicU8;

    #[test]
    fn one_owner_ending_its_capture_keeps_hotkeys_paused_for_the_other() {
        let owners = AtomicU8::new(0);
        assert_eq!(update(&owners, CaptureOwner::Recorder, true), Some(true));
        assert_eq!(update(&owners, CaptureOwner::SettingsWindow, true), None);
        // The settings window reports its idle state every frame.
        assert_eq!(update(&owners, CaptureOwner::SettingsWindow, false), None);
        assert_eq!(update(&owners, CaptureOwner::SettingsWindow, false), None);
        assert_eq!(update(&owners, CaptureOwner::Recorder, false), Some(false));
    }

    #[test]
    fn repeated_reports_from_one_owner_change_nothing() {
        let owners = AtomicU8::new(0);
        assert_eq!(update(&owners, CaptureOwner::Recorder, false), None);
        assert_eq!(
            update(&owners, CaptureOwner::SettingsWindow, true),
            Some(true)
        );
        assert_eq!(update(&owners, CaptureOwner::SettingsWindow, true), None);
        assert_eq!(
            update(&owners, CaptureOwner::SettingsWindow, false),
            Some(false)
        );
    }
}
