//! Shared finite-response budgets for session and request adapters.

pub(super) const CLOCK_MAX: u64 = i64::MAX as u64;

pub(super) fn clock_add(left: u64, right: u64) -> u64 {
    left.min(CLOCK_MAX)
        .saturating_add(right.min(CLOCK_MAX))
        .min(CLOCK_MAX)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ResponsePolicy {
    pub first_output_ms: u64,
    pub content_idle_ms: u64,
    pub hard_response_ms: u64,
}

impl Default for ResponsePolicy {
    fn default() -> Self {
        Self {
            first_output_ms: 20_000,
            content_idle_ms: 1_200,
            hard_response_ms: 90_000,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct ResponseDeadlines {
    pub first: Option<u64>,
    pub idle: Option<u64>,
    pub hard: Option<u64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DeadlineOutcome {
    Pending,
    Complete,
    FirstOutputTimeout,
    HardTimeout,
}

impl ResponseDeadlines {
    pub fn new(now_ms: u64, first_ms: Option<u64>, hard_ms: Option<u64>) -> Self {
        Self {
            first: first_ms.map(|timeout| clock_add(now_ms, timeout)),
            hard: hard_ms.map(|timeout| clock_add(now_ms, timeout)),
            idle: None,
        }
    }

    pub fn observe_output(&mut self, now_ms: u64, idle_ms: Option<u64>) {
        self.first = None;
        self.idle = idle_ms.map(|timeout| clock_add(now_ms, timeout));
    }

    pub fn outcome(self, now_ms: u64, complete: bool) -> DeadlineOutcome {
        let now_ms = now_ms.min(CLOCK_MAX);
        if complete {
            DeadlineOutcome::Complete
        } else if self.hard.is_some_and(|deadline| now_ms >= deadline) {
            DeadlineOutcome::HardTimeout
        } else if self.first.is_some_and(|deadline| now_ms >= deadline) {
            DeadlineOutcome::FirstOutputTimeout
        } else if self.idle.is_some_and(|deadline| now_ms >= deadline) {
            DeadlineOutcome::Complete
        } else {
            DeadlineOutcome::Pending
        }
    }
}
