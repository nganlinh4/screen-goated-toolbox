//! Request-scoped first-output and renewable output-idle deadlines.
use std::cell::RefCell;
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

thread_local! {
    static CURRENT: RefCell<Option<Deadline>> = const { RefCell::new(None) };
}

struct Deadline {
    budget: Duration,
    idle_budget: Duration,
    started: Option<Instant>,
    received: bool,
    cancelled: Option<Arc<AtomicBool>>,
}

/// ureq and its parser run synchronously on the same thread. The guard follows
/// that request, not a pooled connection, and restores any enclosing scope.
pub struct FirstTokenGuard {
    previous: Option<Deadline>,
    _same_thread: PhantomData<Rc<()>>,
}

impl FirstTokenGuard {
    pub fn new(
        streaming: bool,
        timeouts: Option<super::RequestTimeouts>,
        cancelled: &Option<Arc<AtomicBool>>,
    ) -> Self {
        let deadline = streaming.then(|| Deadline {
            budget: timeouts.map_or(super::STREAM_RESPONSE_START_TIMEOUT, |value| {
                value.first_token
            }),
            // Fast first-token expectations must not impose a tight mid-answer
            // limit. Respect explicitly longer allowances without imposing a
            // whole-stream duration cap on actively progressing work.
            idle_budget: timeouts.map_or(super::STREAM_PROGRESS_IDLE_TIMEOUT, |value| {
                super::STREAM_PROGRESS_IDLE_TIMEOUT
                    .max(value.first_token)
                    .max(value.total)
            }),
            started: None,
            received: false,
            cancelled: cancelled.clone(),
        });
        Self {
            previous: CURRENT.with(|current| current.replace(deadline)),
            _same_thread: PhantomData,
        }
    }
}

impl Drop for FirstTokenGuard {
    fn drop(&mut self) {
        CURRENT.with(|current| current.replace(self.previous.take()));
    }
}

pub(super) fn active() -> bool {
    CURRENT.with(|current| current.borrow().is_some())
}

pub(super) fn begin_attempt() {
    CURRENT.with(|current| {
        if let Some(deadline) = current.borrow_mut().as_mut() {
            deadline.started = None;
            deadline.received = false;
        }
    });
}

/// Renew the idle window for each nonempty decoded output chunk, never headers,
/// keepalives, or thinking indicators.
pub fn received() {
    CURRENT.with(|current| {
        if let Some(deadline) = current.borrow_mut().as_mut() {
            deadline.received = true;
            deadline.started = Some(Instant::now());
        }
    });
}

pub(super) fn read_timeout() -> Result<Option<Duration>, ureq::Error> {
    CURRENT.with(|current| {
        let mut current = current.borrow_mut();
        let Some(deadline) = current.as_mut() else {
            return Ok(None);
        };
        if deadline
            .cancelled
            .as_ref()
            .is_some_and(|value| value.load(Ordering::Relaxed))
        {
            // Buffered readers retry Interrupted indefinitely. Cancellation is
            // terminal for this request, so it must escape that retry loop.
            return Err(ureq::Error::Io(std::io::Error::other("Cancelled")));
        }
        let poll = Duration::from_millis(100);
        let budget = if deadline.received {
            deadline.idle_budget
        } else {
            deadline.budget
        };
        let elapsed = deadline.started.get_or_insert_with(Instant::now).elapsed();
        if elapsed >= budget {
            return Err(ureq::Error::Timeout(ureq::Timeout::RecvBody));
        }
        Ok(Some(poll.min(budget - elapsed)))
    })
}

#[cfg(test)]
#[path = "first_token_tests.rs"]
mod tests;
