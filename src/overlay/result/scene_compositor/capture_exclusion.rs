//! Exclude result pixels only during a continuous source-capture session.
use super::protocol::HostCommand;
use std::sync::atomic::{AtomicBool, Ordering};
use windows::Win32::{Foundation::HWND, UI::WindowsAndMessaging::*};

static ENABLED: AtomicBool = AtomicBool::new(false);

pub(crate) fn exclude_from_capture(enabled: bool) {
    if ENABLED.swap(enabled, Ordering::AcqRel) != enabled {
        super::delivery::send_command(snapshot());
    }
}

pub(super) fn snapshot() -> HostCommand {
    HostCommand::CaptureExclusion {
        enabled: ENABLED.load(Ordering::Acquire),
    }
}

pub(super) fn apply(hwnd: HWND, enabled: bool) -> anyhow::Result<()> {
    unsafe {
        SetWindowDisplayAffinity(
            hwnd,
            if enabled {
                WDA_EXCLUDEFROMCAPTURE
            } else {
                WDA_NONE
            },
        )?;
    }
    Ok(())
}

pub(crate) fn capture_is_excluded() -> bool {
    if !ENABLED.load(Ordering::Acquire) {
        return false;
    }
    unsafe {
        let Some(hwnd) = super::supervisor::capture_host() else {
            return false;
        };
        let mut affinity = 0;
        GetWindowDisplayAffinity(hwnd, &mut affinity).is_ok()
            && affinity == WDA_EXCLUDEFROMCAPTURE.0
    }
}
