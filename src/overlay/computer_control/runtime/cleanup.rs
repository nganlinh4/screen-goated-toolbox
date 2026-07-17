//! Session-owned worker and integration cleanup.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use super::super::telemetry::{self, Privacy};
use super::Job;
use super::reader::Reader;

const JOIN_TIMEOUT: Duration = Duration::from_secs(5);

pub(super) fn drain_turn_cleanup_acks(receiver: &mpsc::Receiver<u64>, state: &mut Reader) {
    while let Ok(turn_id) = receiver.try_recv() {
        if state.turn_cleanup_pending == Some(turn_id) {
            state.turn_cleanup_pending = None;
        }
    }
}

#[derive(Default)]
struct EndSnapshot {
    connection_generation: u32,
    reconnect_total: u32,
    turn_mode: &'static str,
    pending_tool: Option<String>,
    history_entries: usize,
}

impl EndSnapshot {
    fn capture(&mut self, state: &Reader) {
        self.connection_generation = state.connection_generation;
        self.reconnect_total = state.reconnect_total;
        self.turn_mode = state.turn_mode.as_str();
        self.pending_tool.clone_from(&state.pending.id);
        self.history_entries = state.history.len();
    }
}

pub(super) struct SessionCleanup {
    stop: Arc<AtomicBool>,
    executor_sender: Option<mpsc::Sender<Job>>,
    pending_cancel: Option<Arc<AtomicBool>>,
    workers: Vec<(&'static str, JoinHandle<()>)>,
    mcp_started: bool,
    recorded: bool,
    cleanup_complete: bool,
    snapshot: EndSnapshot,
}

impl SessionCleanup {
    pub(super) fn new(stop: Arc<AtomicBool>) -> Self {
        Self {
            stop,
            executor_sender: None,
            pending_cancel: None,
            workers: Vec::new(),
            mcp_started: false,
            recorded: false,
            cleanup_complete: true,
            snapshot: EndSnapshot::default(),
        }
    }

    pub(super) fn register_worker(&mut self, name: &'static str, worker: JoinHandle<()>) {
        self.workers.push((name, worker));
    }

    pub(super) fn register_executor(&mut self, sender: mpsc::Sender<Job>, worker: JoinHandle<()>) {
        self.executor_sender = Some(sender);
        self.register_worker("action_executor", worker);
    }

    pub(super) fn mark_mcp_started(&mut self) {
        self.mcp_started = true;
    }

    pub(super) fn track_pending(&mut self, state: &Reader) {
        self.pending_cancel.clone_from(&state.pending.cancel);
        self.snapshot.capture(state);
    }

    pub(super) fn finish(mut self, state: &mut Reader, reason: &str) {
        state.pending.request_cancel();
        self.track_pending(state);
        self.teardown();
        self.record(reason);
    }

    fn teardown(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(cancel) = self.pending_cancel.take() {
            cancel.store(true, Ordering::SeqCst);
        }
        self.executor_sender.take();
        for (name, worker) in self.workers.drain(..) {
            if !join_bounded(name, worker) {
                self.cleanup_complete = false;
            }
        }
        if self.mcp_started {
            super::super::mcp::disconnect_all();
            self.mcp_started = false;
        }
    }

    fn record(&mut self, reason: &str) {
        if self.recorded {
            return;
        }
        telemetry::event(
            "session_end",
            "runtime",
            Privacy::Safe,
            serde_json::json!({
                "reason": reason,
                "connection_generation": self.snapshot.connection_generation,
                "reconnect_total": self.snapshot.reconnect_total,
                "turn_mode": self.snapshot.turn_mode,
                "pending_tool": self.snapshot.pending_tool,
                "history_entries": self.snapshot.history_entries,
                "runtime_cleanup_complete": self.cleanup_complete,
            }),
        );
        self.recorded = true;
    }
}

impl Drop for SessionCleanup {
    fn drop(&mut self) {
        if self.recorded {
            return;
        }
        self.teardown();
        self.record("cleanup_guard_drop");
    }
}

fn join_bounded(name: &'static str, worker: JoinHandle<()>) -> bool {
    let started = Instant::now();
    while !worker.is_finished() && started.elapsed() < JOIN_TIMEOUT {
        std::thread::sleep(Duration::from_millis(10));
    }
    if !worker.is_finished() {
        telemetry::typed_error(
            "ERR_RUNTIME_WORKER_SHUTDOWN_TIMEOUT",
            "runtime",
            "a session-owned worker did not stop before the cleanup deadline",
            serde_json::json!({"worker": name, "timeout_ms": JOIN_TIMEOUT.as_millis()}),
        );
        return false;
    }
    if worker.join().is_err() {
        telemetry::typed_error(
            "ERR_RUNTIME_WORKER_PANICKED",
            "runtime",
            "a session-owned worker panicked during shutdown",
            serde_json::json!({"worker": name}),
        );
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleanup_ack_clears_only_the_matching_pending_turn() {
        let (tx, rx) = mpsc::channel();
        let mut state = Reader {
            turn_cleanup_pending: Some(8),
            ..Reader::default()
        };
        tx.send(7).unwrap();
        drain_turn_cleanup_acks(&rx, &mut state);
        assert_eq!(state.turn_cleanup_pending, Some(8));

        tx.send(8).unwrap();
        drain_turn_cleanup_acks(&rx, &mut state);
        assert_eq!(state.turn_cleanup_pending, None);
    }

    #[test]
    fn dropping_cleanup_stops_and_joins_registered_workers() {
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let worker_stopped = Arc::clone(&stopped);
        let worker = std::thread::spawn(move || {
            while !worker_stop.load(Ordering::SeqCst) {
                std::thread::yield_now();
            }
            worker_stopped.store(true, Ordering::SeqCst);
        });
        let mut cleanup = SessionCleanup::new(Arc::clone(&stop));
        cleanup.register_worker("test_worker", worker);
        drop(cleanup);
        assert!(stop.load(Ordering::SeqCst));
        assert!(stopped.load(Ordering::SeqCst));
    }
}
