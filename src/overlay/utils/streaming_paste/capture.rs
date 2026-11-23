//! Start-time window identity and a watchdog for potentially stalled UIA capture.
use super::Shared;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Instant;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

pub(super) fn report_unavailable(abort: &AtomicBool) {
    if !abort.load(Ordering::SeqCst) {
        crate::overlay::auto_copy_badge::show_error_notification(
            "Streaming auto-paste unavailable",
        );
    }
}

pub(super) fn report_failure(shared: &Shared) {
    if !shared.failure_reported.swap(true, Ordering::SeqCst) {
        report_unavailable(&shared.abort);
    }
}

pub(super) fn may_fallback(error: &anyhow::Error, allowed: bool) -> bool {
    allowed && !error.is::<super::editor::UnsafeCapture>()
}

pub(super) fn mutation_is_uncertain(error: &anyhow::Error, finishing: bool) -> bool {
    finishing || !error.is::<super::editor::BeforeMutation>()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct WindowIdentity {
    window: usize,
    process: u32,
    thread: u32,
}

impl WindowIdentity {
    pub(super) fn capture(window: HWND) -> Option<Self> {
        if window.is_invalid() {
            return None;
        }
        let mut process = 0;
        let thread = unsafe { GetWindowThreadProcessId(window, Some(&mut process)) };
        (process != 0 && thread != 0).then_some(Self {
            window: window.0 as usize,
            process,
            thread,
        })
    }

    pub(super) fn hwnd(self) -> HWND {
        HWND(self.window as *mut _)
    }

    pub(super) fn is_current_foreground(self) -> bool {
        let foreground = unsafe { GetForegroundWindow() };
        admits_identity(self, Self::capture(foreground))
    }
}

fn admits_identity(expected: WindowIdentity, observed: Option<WindowIdentity>) -> bool {
    observed.is_some_and(|observed| expected == observed)
}

pub(super) fn may_capture(
    shared: &Shared,
    abort: &AtomicBool,
    deadline: Option<Instant>,
    identity: Option<WindowIdentity>,
) -> bool {
    let state = shared.state.lock().unwrap();
    !state.expired
        && !state.overflow
        && !abort.load(Ordering::SeqCst)
        && deadline.is_none_or(|deadline| Instant::now() < deadline)
        && identity.is_some_and(WindowIdentity::is_current_foreground)
}

pub(super) fn monitor(shared: Arc<Shared>, deadline: Instant) {
    let mut state = shared.state.lock().unwrap_or_else(|e| e.into_inner());
    while state.capture_pending && !state.done && !state.expired {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            state.expired = true;
            state.accepting = false;
            state.queue.clear();
            shared.changed.notify_all();
            drop(state);
            report_failure(&shared);
            return;
        }
        state = shared
            .changed
            .wait_timeout(state, remaining)
            .unwrap_or_else(|e| e.into_inner())
            .0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_provider_can_fallback_but_lost_ownership_cannot() {
        let unsupported = anyhow::anyhow!("text pattern unsupported");
        let unsafe_capture = anyhow::Error::new(super::super::editor::UnsafeCapture);
        assert!(may_fallback(&unsupported, true));
        assert!(!may_fallback(&unsupported, false));
        assert!(!may_fallback(&unsafe_capture, true));
    }

    #[test]
    fn failed_select_is_reported_without_unverified_replay() {
        let rejected_selection = anyhow::anyhow!("provider rejected Select");
        let before_mutation =
            anyhow::anyhow!("target changed").context(super::super::editor::BeforeMutation);
        assert!(mutation_is_uncertain(&rejected_selection, false));
        assert!(!mutation_is_uncertain(&before_mutation, false));
        let shared = Shared::default();
        shared.abort.store(true, Ordering::SeqCst);
        report_failure(&shared);
        assert!(shared.failure_reported.load(Ordering::SeqCst));
    }

    #[test]
    fn changed_focus_is_not_eligible_for_keyboard_fallback() {
        let error = super::super::editor::require_ownership(false).unwrap_err();
        assert!(error.is::<super::super::editor::UnsafeCapture>());
        assert!(!may_fallback(&error, true));
    }

    #[test]
    fn reused_handle_with_different_process_or_thread_is_not_the_original_window() {
        let original = WindowIdentity {
            window: 42,
            process: 10,
            thread: 11,
        };
        assert!(!admits_identity(
            original,
            Some(WindowIdentity {
                process: 20,
                ..original
            })
        ));
        assert!(!admits_identity(
            original,
            Some(WindowIdentity {
                thread: 21,
                ..original
            })
        ));
        assert!(!admits_identity(
            original,
            Some(WindowIdentity {
                window: 43,
                ..original
            })
        ));
        assert!(!admits_identity(original, None));
        assert!(admits_identity(original, Some(original)));
    }

    #[test]
    fn expired_capture_is_revoked_without_waiting_for_its_worker() {
        let shared = Arc::new(Shared::default());
        {
            let mut state = shared.state.lock().unwrap();
            state.capture_pending = true;
            state.accepting = true;
        }
        shared
            .abort
            .store(true, std::sync::atomic::Ordering::SeqCst);
        monitor(shared.clone(), Instant::now());
        let state = shared.state.lock().unwrap();
        assert!(state.expired && !state.accepting);
        assert!(
            shared
                .failure_reported
                .load(std::sync::atomic::Ordering::SeqCst)
        );
    }
}
