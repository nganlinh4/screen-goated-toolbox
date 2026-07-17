//! Bounded readiness policy for an already-installed browser extension.

use std::cell::RefCell;
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

const FULL_RECONNECT_CYCLE: Duration = Duration::from_secs(16);
const STALE_INSTALL_PROBE: Duration = Duration::from_secs(2);
const BRIDGE_STARTUP_WINDOW: Duration = Duration::from_secs(20);
pub(super) const CONNECTION_POLL: Duration = Duration::from_millis(100);

static BRIDGE_STARTED_AT: OnceLock<Instant> = OnceLock::new();
thread_local! {
    static PREFLIGHT_CANCEL: RefCell<Vec<Arc<AtomicBool>>> = const { RefCell::new(Vec::new()) };
    static REQUEST_DEADLINES: RefCell<Vec<Instant>> = const { RefCell::new(Vec::new()) };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WaitOutcome {
    Ready,
    Cancelled,
    TimedOut,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum StartupWait {
    None,
    RecentPeer(Duration),
    StalePeer(Duration),
}

impl StartupWait {
    pub(super) fn duration(self) -> Duration {
        match self {
            Self::None => Duration::ZERO,
            Self::RecentPeer(duration) | Self::StalePeer(duration) => duration,
        }
    }

    pub(super) fn reason(self) -> &'static str {
        match self {
            Self::None => "not_expected",
            Self::RecentPeer(_) => "recent_peer",
            Self::StalePeer(_) => "stale_peer_probe",
        }
    }
}

pub(in crate::overlay::computer_control) struct ConnectionPreflight {
    _not_send: PhantomData<Rc<()>>,
}

pub(in crate::overlay::computer_control) struct RequestDeadline {
    _not_send: PhantomData<Rc<()>>,
}

impl Drop for RequestDeadline {
    fn drop(&mut self) {
        REQUEST_DEADLINES.with(|deadlines| {
            deadlines.borrow_mut().pop();
        });
    }
}

impl Drop for ConnectionPreflight {
    fn drop(&mut self) {
        PREFLIGHT_CANCEL.with(|tokens| {
            tokens.borrow_mut().pop();
        });
    }
}

pub(super) fn mark_bridge_start() {
    BRIDGE_STARTED_AT.get_or_init(Instant::now);
}

pub(super) fn bridge_startup_plausible() -> bool {
    BRIDGE_STARTED_AT
        .get()
        .is_some_and(|started| started.elapsed() <= BRIDGE_STARTUP_WINDOW)
}

pub(super) fn existing_install_wait(recent: bool, startup_plausible: bool) -> Duration {
    if recent || startup_plausible {
        FULL_RECONNECT_CYCLE
    } else {
        STALE_INSTALL_PROBE
    }
}

pub(super) fn startup_wait(connected: bool, ever_connected: bool, recent: bool) -> StartupWait {
    if connected || !ever_connected {
        StartupWait::None
    } else if recent {
        StartupWait::RecentPeer(FULL_RECONNECT_CYCLE)
    } else {
        StartupWait::StalePeer(STALE_INSTALL_PROBE)
    }
}

pub(super) fn enter_preflight(cancel: &Arc<AtomicBool>) -> ConnectionPreflight {
    PREFLIGHT_CANCEL.with(|tokens| tokens.borrow_mut().push(Arc::clone(cancel)));
    ConnectionPreflight {
        _not_send: PhantomData,
    }
}

pub(super) fn enter_request_deadline(duration: Duration) -> RequestDeadline {
    REQUEST_DEADLINES.with(|deadlines| {
        deadlines.borrow_mut().push(Instant::now() + duration);
    });
    RequestDeadline {
        _not_send: PhantomData,
    }
}

pub(super) fn bounded_request_timeout(default: Duration) -> Duration {
    bounded_timeout(
        default,
        REQUEST_DEADLINES.with(|deadlines| deadlines.borrow().iter().copied().min()),
        Instant::now(),
    )
}

fn bounded_timeout(default: Duration, deadline: Option<Instant>, now: Instant) -> Duration {
    deadline
        .map(|deadline| default.min(deadline.saturating_duration_since(now)))
        .unwrap_or(default)
}

pub(super) fn preflight_active() -> bool {
    PREFLIGHT_CANCEL.with(|tokens| !tokens.borrow().is_empty())
}

pub(super) fn current_cancel() -> Option<Arc<AtomicBool>> {
    PREFLIGHT_CANCEL.with(|tokens| tokens.borrow().last().cloned())
}

pub(super) fn action_cancelled() -> bool {
    current_cancel().is_some_and(|token| token.load(Ordering::SeqCst))
}

pub(super) fn pause_cancelled(duration: Duration) -> bool {
    let cancel = current_cancel();
    wait_for_connection(duration, cancel.as_deref(), || false) == WaitOutcome::Cancelled
}

pub(super) fn wait_for_connection(
    timeout: Duration,
    cancel: Option<&AtomicBool>,
    connected: impl FnMut() -> bool,
) -> WaitOutcome {
    wait_for_connection_with(timeout, cancel, connected, std::thread::sleep)
}

fn wait_for_connection_with(
    timeout: Duration,
    cancel: Option<&AtomicBool>,
    mut connected: impl FnMut() -> bool,
    mut pause: impl FnMut(Duration),
) -> WaitOutcome {
    let deadline = Instant::now() + timeout;
    loop {
        if cancel.is_some_and(|token| token.load(Ordering::SeqCst)) {
            return WaitOutcome::Cancelled;
        }
        if connected() {
            return WaitOutcome::Ready;
        }
        let now = Instant::now();
        if now >= deadline {
            return WaitOutcome::TimedOut;
        }
        pause(CONNECTION_POLL.min(deadline.saturating_duration_since(now)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn recent_or_startup_plausible_install_gets_one_full_reconnect_cycle() {
        assert_eq!(existing_install_wait(true, false), FULL_RECONNECT_CYCLE);
        assert_eq!(existing_install_wait(false, true), FULL_RECONNECT_CYCLE);
        assert_eq!(existing_install_wait(true, true), FULL_RECONNECT_CYCLE);
    }

    #[test]
    fn stale_install_keeps_the_brief_probe() {
        assert_eq!(existing_install_wait(false, false), STALE_INSTALL_PROBE);
        assert!(STALE_INSTALL_PROBE < FULL_RECONNECT_CYCLE);
    }

    #[test]
    fn startup_waits_only_for_a_known_peer() {
        assert_eq!(startup_wait(true, true, true), StartupWait::None);
        assert_eq!(startup_wait(false, false, false), StartupWait::None);
        assert_eq!(
            startup_wait(false, true, true),
            StartupWait::RecentPeer(FULL_RECONNECT_CYCLE)
        );
        assert_eq!(
            startup_wait(false, true, false),
            StartupWait::StalePeer(STALE_INSTALL_PROBE)
        );
    }

    #[test]
    fn cancellation_interrupts_readiness_on_the_next_poll_without_sleeping() {
        let cancel = AtomicBool::new(false);
        let polls = Cell::new(0_u32);
        let outcome = wait_for_connection_with(
            Duration::from_secs(1),
            Some(&cancel),
            || false,
            |_| {
                polls.set(polls.get() + 1);
                cancel.store(true, Ordering::SeqCst);
            },
        );

        assert_eq!(outcome, WaitOutcome::Cancelled);
        assert_eq!(polls.get(), 1);
    }

    #[test]
    fn cancelled_preflight_wins_over_a_ready_transport() {
        let cancel = AtomicBool::new(true);
        assert_eq!(
            wait_for_connection_with(Duration::ZERO, Some(&cancel), || true, |_| {}),
            WaitOutcome::Cancelled
        );
    }

    #[test]
    fn preflight_lease_is_scoped_to_the_dispatch_thread() {
        let cancel = Arc::new(AtomicBool::new(false));
        assert!(!preflight_active());
        assert!(current_cancel().is_none());
        {
            let _lease = enter_preflight(&cancel);
            assert!(preflight_active());
            assert!(!action_cancelled());
            cancel.store(true, Ordering::SeqCst);
            assert!(action_cancelled());
        }
        assert!(!preflight_active());
        assert!(current_cancel().is_none());
    }

    #[test]
    fn request_timeout_never_outlives_the_nearest_deadline() {
        let now = Instant::now();
        assert_eq!(
            bounded_timeout(
                Duration::from_secs(15),
                Some(now + Duration::from_secs(2)),
                now,
            ),
            Duration::from_secs(2)
        );
        assert_eq!(
            bounded_timeout(Duration::from_secs(3), None, now),
            Duration::from_secs(3)
        );
        assert_eq!(
            bounded_timeout(Duration::from_secs(3), Some(now), now),
            Duration::ZERO
        );
    }
}
