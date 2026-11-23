//! Serialize whole sessions while keeping provider callbacks independent of editor I/O.
use std::collections::VecDeque;
use std::sync::{
    Condvar, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

#[derive(Default)]
pub(super) struct Gate {
    queue: Mutex<VecDeque<u64>>,
    changed: Condvar,
}

pub(super) struct Ticket<'a> {
    gate: &'a Gate,
    id: u64,
}

impl Gate {
    pub(super) fn reserve(&self, id: u64, abort: &AtomicBool) -> Option<Ticket<'_>> {
        let mut queue = self.queue.lock().unwrap_or_else(|e| e.into_inner());
        if abort.load(Ordering::SeqCst) || queue.len() >= 2 {
            return None;
        }
        queue.push_back(id);
        Some(Ticket { gate: self, id })
    }
}

impl Ticket<'_> {
    pub(super) fn wait(&self, abort: &AtomicBool, expired: impl Fn() -> bool) -> bool {
        let mut queue = self.gate.queue.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if abort.load(Ordering::SeqCst) || expired() {
                return false;
            }
            if queue.front() == Some(&self.id) {
                return true;
            }
            queue = self
                .gate
                .changed
                .wait_timeout(queue, Duration::from_millis(50))
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
    }
}

impl Drop for Ticket<'_> {
    fn drop(&mut self) {
        let mut queue = self.gate.queue.lock().unwrap_or_else(|e| e.into_inner());
        queue.retain(|id| *id != self.id);
        self.gate.changed.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, mpsc};

    #[test]
    fn cancelled_late_worker_does_not_claim_or_revoke_new_session() {
        let gate = Gate::default();
        let old_abort = AtomicBool::new(true);
        let new_abort = AtomicBool::new(false);
        let new = gate.reserve(2, &new_abort).unwrap();
        assert!(gate.reserve(1, &old_abort).is_none());
        assert!(new.wait(&new_abort, || false));
    }

    #[test]
    fn third_session_is_rejected_without_displacing_reserved_sessions() {
        let gate = Gate::default();
        let abort = AtomicBool::new(false);
        let first = gate.reserve(1, &abort).unwrap();
        let second = gate.reserve(2, &abort).unwrap();
        assert!(gate.reserve(3, &abort).is_none());
        assert!(first.wait(&abort, || false));
        drop(first);
        assert!(second.wait(&abort, || false));
        drop(second);
        assert!(gate.reserve(3, &abort).is_some());
    }

    #[test]
    fn waiting_session_can_expire_when_an_earlier_writer_stalls() {
        let gate = Gate::default();
        let abort = AtomicBool::new(false);
        let _first = gate.reserve(1, &abort).unwrap();
        let second = gate.reserve(2, &abort).unwrap();
        assert!(!second.wait(&abort, || true));
    }

    #[test]
    fn stopped_session_drains_before_new_session_can_dispatch() {
        let gate = Arc::new(Gate::default());
        let abort = AtomicBool::new(false);
        let old = gate.reserve(1, &abort).unwrap();
        let next = gate.reserve(2, &abort).unwrap();
        let (started_tx, started_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        std::thread::scope(|scope| {
            scope.spawn(move || {
                started_tx.send(()).unwrap();
                assert!(next.wait(&abort, || false));
                done_tx.send(()).unwrap();
            });
            started_rx.recv().unwrap();
            assert!(done_rx.try_recv().is_err());
            drop(old);
            done_rx.recv().unwrap();
        });
    }
}
