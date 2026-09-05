use std::collections::VecDeque;
use std::sync::{LazyLock, Mutex};

pub(super) const CONVERGENCE_TIMEOUT_MS: u64 = 1_500;
const MAX_PENDING_BATCHES: usize = 4096;

#[derive(Default)]
struct Reconciliation {
    issued: u64,
    applied: u64,
    pending: VecDeque<(u64, u64)>,
    overflowed: bool,
}

impl Reconciliation {
    fn queue(&mut self, now: u64) -> u64 {
        self.issued += 1;
        if self.pending.len() < MAX_PENDING_BATCHES {
            self.pending.push_back((self.issued, now));
        } else {
            self.overflowed = true;
        }
        self.issued
    }

    fn acknowledge(&mut self, revision: u64) {
        if revision <= self.applied || revision > self.issued {
            return;
        }
        self.applied = revision;
        while self.pending.front().is_some_and(|(id, _)| *id <= revision) {
            self.pending.pop_front();
        }
        if revision == self.issued {
            self.overflowed = false;
        }
    }

    fn converging(&self, now: u64) -> bool {
        !self.overflowed
            && self
                .pending
                .front()
                .is_none_or(|(_, queued)| now.saturating_sub(*queued) <= CONVERGENCE_TIMEOUT_MS)
    }

    fn reset(&mut self) {
        // Keep revision identity monotonic across renderer generations.
        self.applied = self.issued;
        self.pending.clear();
        self.overflowed = false;
    }
}

static TRACKER: LazyLock<Mutex<Reconciliation>> =
    LazyLock::new(|| Mutex::new(Reconciliation::default()));

pub(super) fn advance_revision() -> u64 {
    TRACKER.lock().unwrap().queue(super::supervisor::now_ms())
}

pub(super) fn acknowledge_revision(revision: u64) {
    TRACKER.lock().unwrap().acknowledge(revision);
}

pub(super) fn check_convergence(now_ms: u64) -> bool {
    TRACKER.lock().unwrap().converging(now_ms)
}

pub(super) fn reset() {
    TRACKER.lock().unwrap().reset();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traffic_and_partial_progress_do_not_extend_outstanding_deadlines() {
        let mut state = Reconciliation::default();
        let first = state.queue(0);
        state.queue(100);
        state.queue(1000);
        state.acknowledge(first);
        assert!(state.converging(1600));
        assert!(!state.converging(1601));
    }

    #[test]
    fn applied_removal_is_not_kept_pending_by_newer_streaming() {
        let mut state = Reconciliation::default();
        let removal = state.queue(0);
        let stream = state.queue(1000);
        state.acknowledge(removal);
        assert!(state.converging(1501));
        assert!(!state.converging(2501));
        state.acknowledge(stream);
        assert!(state.converging(10000));
    }

    #[test]
    fn duplicate_stale_and_future_acknowledgements_cannot_mask_failure() {
        let mut state = Reconciliation::default();
        let first = state.queue(0);
        state.acknowledge(first);
        state.queue(100);
        for revision in [0, first, 999] {
            state.acknowledge(revision);
        }
        assert!(!state.converging(1601));
    }

    #[test]
    fn coalesced_acknowledgement_covers_prior_batches() {
        let mut state = Reconciliation::default();
        state.queue(0);
        let newest = state.queue(10);
        state.acknowledge(newest);
        assert!(state.pending.is_empty());
        assert!(state.converging(10000));
    }

    #[test]
    fn reset_does_not_reuse_revision_identity() {
        let mut state = Reconciliation::default();
        let old = state.queue(0);
        state.reset();
        assert!(state.queue(10) > old);
        state.acknowledge(old);
        assert!(!state.converging(1511));
    }

    #[test]
    fn pending_storage_is_bounded_and_overflow_requires_recovery() {
        let mut state = Reconciliation::default();
        for _ in 0..=MAX_PENDING_BATCHES {
            state.queue(0);
        }
        assert_eq!(state.pending.len(), MAX_PENDING_BATCHES);
        assert!(!state.converging(0));
    }
}
