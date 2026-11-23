//! Bounded re-validation of the keyboard target during one typing batch.
//!
//! The full target guard (window instance, UIA focused element, browser
//! identity) is too slow to run before every UTF-16 unit. A lease runs it
//! at the start and again every [`FULL_CHECK_UNITS`] units or
//! [`FULL_CHECK_INTERVAL`]. Between full checks, each unit requires the same
//! foreground window and focused child window that bracketed the last full
//! check. Any mismatch forces a full check instead of a silent pass.

use std::time::{Duration, Instant};

use anyhow::Result;

const FULL_CHECK_UNITS: usize = 32;
const FULL_CHECK_INTERVAL: Duration = Duration::from_millis(250);

/// The foreground window and the focused window of its GUI thread.
pub(super) type FocusAnchor = (usize, usize);

pub(super) struct TargetLease<Full, Probe> {
    full_check: Full,
    probe: Probe,
    anchor: Option<FocusAnchor>,
    units_since_full: usize,
    full_at: Instant,
}

impl<Full, Probe> TargetLease<Full, Probe>
where
    Full: FnMut() -> Result<()>,
    Probe: FnMut() -> FocusAnchor,
{
    pub(super) fn new(full_check: Full, probe: Probe) -> Self {
        Self {
            full_check,
            probe,
            anchor: None,
            units_since_full: 0,
            full_at: Instant::now(),
        }
    }

    /// Admit one more key edge pair, or fail if the target changed.
    pub(super) fn check(&mut self) -> Result<()> {
        if let Some(anchor) = self.anchor
            && self.units_since_full < FULL_CHECK_UNITS
            && self.full_at.elapsed() < FULL_CHECK_INTERVAL
            && (self.probe)() == anchor
        {
            self.units_since_full += 1;
            return Ok(());
        }
        let before = (self.probe)();
        (self.full_check)()?;
        let after = (self.probe)();
        // Anchor only a focus state that held across the whole full check.
        self.anchor = (before == after).then_some(after);
        self.units_since_full = 1;
        self.full_at = Instant::now();
        Ok(())
    }
}

pub(super) fn focus_anchor() -> FocusAnchor {
    use windows::Win32::UI::WindowsAndMessaging::{
        GUITHREADINFO, GetForegroundWindow, GetGUIThreadInfo, GetWindowThreadProcessId,
    };
    let foreground = unsafe { GetForegroundWindow() };
    let thread = unsafe { GetWindowThreadProcessId(foreground, None) };
    let mut info = GUITHREADINFO {
        cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
        ..Default::default()
    };
    let focus = if thread != 0 && unsafe { GetGUIThreadInfo(thread, &mut info) }.is_ok() {
        info.hwndFocus.0 as usize
    } else {
        0
    };
    (foreground.0 as usize, focus)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn full_guard_runs_once_per_window_of_units_while_focus_holds() {
        let full = Cell::new(0);
        let mut lease = TargetLease::new(
            || {
                full.set(full.get() + 1);
                Ok(())
            },
            || (7, 8),
        );
        for _ in 0..FULL_CHECK_UNITS * 3 {
            lease.check().unwrap();
        }
        assert_eq!(full.get(), 3);
    }

    #[test]
    fn a_focus_change_forces_the_full_guard_before_the_next_unit() {
        let focus = Cell::new(8);
        let full = Cell::new(0);
        let mut lease = TargetLease::new(
            || {
                full.set(full.get() + 1);
                anyhow::ensure!(focus.get() == 8, "focus left the expected element");
                Ok(())
            },
            || (7, focus.get()),
        );
        lease.check().unwrap();
        lease.check().unwrap();
        assert_eq!(full.get(), 1);
        focus.set(9);
        assert!(
            lease.check().is_err(),
            "the moved focus must not be typed into"
        );
        assert_eq!(full.get(), 2);
    }

    #[test]
    fn focus_that_moved_during_the_full_guard_is_never_anchored() {
        let probes = Cell::new(0);
        let full = Cell::new(0);
        let mut lease = TargetLease::new(
            || {
                full.set(full.get() + 1);
                Ok(())
            },
            || {
                probes.set(probes.get() + 1);
                (7, probes.get())
            },
        );
        lease.check().unwrap();
        lease.check().unwrap();
        assert_eq!(
            full.get(),
            2,
            "an unstable focus keeps every unit fully checked"
        );
    }
}
