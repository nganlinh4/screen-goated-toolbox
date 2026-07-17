//! Cancellation-aware wait for one correlated browser-bridge reply.

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use serde_json::Value;

const REPLY_POLL: Duration = Duration::from_millis(50);

pub(super) struct PendingReply {
    reply: Sender<Value>,
    cancel: Option<Arc<AtomicBool>>,
    deadline: Instant,
}

impl PendingReply {
    pub(super) fn new(
        reply: Sender<Value>,
        cancel: Option<Arc<AtomicBool>>,
        deadline: Instant,
    ) -> Self {
        Self {
            reply,
            cancel,
            deadline,
        }
    }

    pub(super) fn deliver(self, value: Value) {
        let _ = self.reply.send(value);
    }

    fn inactive(&self, now: Instant) -> bool {
        now >= self.deadline || cancelled(self.cancel.as_deref())
    }
}

pub(super) fn prune_inactive(pending: &mut HashMap<u64, PendingReply>, now: Instant) {
    pending.retain(|_, reply| !reply.inactive(now));
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RequestWaitError {
    CancelledBeforeDispatch,
    UnavailableBeforeDispatch,
    CancelledWhileWaiting,
    TimedOut,
    Closed,
}

impl fmt::Display for RequestWaitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::CancelledBeforeDispatch => "browser request cancelled before dispatch",
            Self::UnavailableBeforeDispatch => "browser request unavailable before dispatch",
            Self::CancelledWhileWaiting => "browser request cancelled while awaiting its reply",
            Self::TimedOut => "browser request timed out",
            Self::Closed => "browser bridge closed",
        })
    }
}

impl std::error::Error for RequestWaitError {}

pub(super) fn cancellation_effect(error: &anyhow::Error) -> Option<bool> {
    match error.downcast_ref::<RequestWaitError>()? {
        RequestWaitError::CancelledBeforeDispatch => Some(false),
        RequestWaitError::CancelledWhileWaiting => Some(true),
        RequestWaitError::UnavailableBeforeDispatch
        | RequestWaitError::TimedOut
        | RequestWaitError::Closed => None,
    }
}

pub(super) fn dispatch_effect(error: &anyhow::Error) -> Option<bool> {
    Some(match error.downcast_ref::<RequestWaitError>()? {
        RequestWaitError::CancelledBeforeDispatch | RequestWaitError::UnavailableBeforeDispatch => {
            false
        }
        RequestWaitError::CancelledWhileWaiting
        | RequestWaitError::TimedOut
        | RequestWaitError::Closed => true,
    })
}

pub(super) fn cancelled_before_dispatch() -> anyhow::Error {
    RequestWaitError::CancelledBeforeDispatch.into()
}

pub(super) fn unavailable_before_dispatch() -> anyhow::Error {
    RequestWaitError::UnavailableBeforeDispatch.into()
}

pub(super) fn closed_after_dispatch() -> anyhow::Error {
    RequestWaitError::Closed.into()
}

pub(super) fn ensure_dispatch_allowed(cancel: Option<&AtomicBool>) -> anyhow::Result<()> {
    if cancelled(cancel) {
        Err(cancelled_before_dispatch())
    } else {
        Ok(())
    }
}

pub(super) fn receive(
    receiver: &Receiver<Value>,
    deadline: Instant,
    cancel: Option<&AtomicBool>,
) -> anyhow::Result<Value> {
    receive_with(deadline, cancel, |wait| receiver.recv_timeout(wait)).map_err(Into::into)
}

fn receive_with(
    deadline: Instant,
    cancel: Option<&AtomicBool>,
    mut receive: impl FnMut(Duration) -> Result<Value, RecvTimeoutError>,
) -> Result<Value, RequestWaitError> {
    loop {
        if cancelled(cancel) {
            return Err(RequestWaitError::CancelledWhileWaiting);
        }
        let now = Instant::now();
        if now >= deadline {
            return Err(RequestWaitError::TimedOut);
        }
        match receive(REPLY_POLL.min(deadline.saturating_duration_since(now))) {
            Ok(reply) => return Ok(reply),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) if cancelled(cancel) => {
                return Err(RequestWaitError::CancelledWhileWaiting);
            }
            Err(RecvTimeoutError::Disconnected) if Instant::now() >= deadline => {
                return Err(RequestWaitError::TimedOut);
            }
            Err(RecvTimeoutError::Disconnected) => return Err(RequestWaitError::Closed),
        }
    }
}

fn cancelled(cancel: Option<&AtomicBool>) -> bool {
    cancel.is_some_and(|token| token.load(Ordering::SeqCst))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancellation_after_dispatch_interrupts_on_the_next_reply_poll() {
        let cancel = AtomicBool::new(false);
        let mut polls = 0;
        let result = receive_with(
            Instant::now() + Duration::from_secs(1),
            Some(&cancel),
            |_| {
                polls += 1;
                cancel.store(true, Ordering::SeqCst);
                Err(RecvTimeoutError::Timeout)
            },
        );

        assert_eq!(result, Err(RequestWaitError::CancelledWhileWaiting));
        assert_eq!(polls, 1);
    }

    #[test]
    fn cancellation_wins_before_a_late_reply_can_be_consumed() {
        let cancel = AtomicBool::new(true);
        let result = receive_with(
            Instant::now() + Duration::from_secs(1),
            Some(&cancel),
            |_| panic!("a cancelled request must not wait for or consume a reply"),
        );

        assert_eq!(result, Err(RequestWaitError::CancelledWhileWaiting));
    }

    #[test]
    fn cancellation_metadata_distinguishes_pre_dispatch_from_unknown_effect() {
        assert_eq!(
            cancellation_effect(&cancelled_before_dispatch()),
            Some(false)
        );
        let during: anyhow::Error = RequestWaitError::CancelledWhileWaiting.into();
        assert_eq!(cancellation_effect(&during), Some(true));
        assert_eq!(dispatch_effect(&cancelled_before_dispatch()), Some(false));
        assert_eq!(dispatch_effect(&during), Some(true));
    }

    #[test]
    fn cancellation_after_preflight_prevents_the_next_dispatch() {
        let cancel = Arc::new(AtomicBool::new(false));
        let _preflight = super::super::readiness::enter_preflight(&cancel);
        cancel.store(true, Ordering::SeqCst);
        let scoped = super::super::readiness::current_cancel();

        let error = ensure_dispatch_allowed(scoped.as_deref()).unwrap_err();

        assert_eq!(cancellation_effect(&error), Some(false));
    }

    #[test]
    fn pending_replies_are_pruned_on_cancellation_or_deadline() {
        let now = Instant::now();
        let active = Arc::new(AtomicBool::new(false));
        let cancelled = Arc::new(AtomicBool::new(true));
        let mut pending = HashMap::new();
        for (id, token, deadline) in [
            (1, Arc::clone(&active), now + Duration::from_secs(1)),
            (2, cancelled, now + Duration::from_secs(1)),
            (3, Arc::clone(&active), now),
        ] {
            let (tx, _rx) = std::sync::mpsc::channel();
            pending.insert(id, PendingReply::new(tx, Some(token), deadline));
        }

        prune_inactive(&mut pending, now);

        assert_eq!(pending.len(), 1);
        assert!(pending.contains_key(&1));
    }
}
