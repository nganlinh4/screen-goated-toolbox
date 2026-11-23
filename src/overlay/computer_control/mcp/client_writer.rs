//! One bounded frame writer; an interrupted partial frame retires its transport.
use super::client_process::ServerProcess;
use crate::process_job::Input;
use serde_json::Value;
use std::io::{self, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const POLL: Duration = Duration::from_millis(20);
const QUEUED: u8 = 0;
const STARTED: u8 = 1;
const CANCELLED: u8 = 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum FailureKind {
    Cancelled,
    Timeout,
    Transport,
}

pub(super) struct Failure {
    pub(super) kind: FailureKind,
    pub(super) started: bool,
    pub(super) message: String,
}

#[derive(Default)]
struct Ticket(AtomicU8);
impl Ticket {
    fn begin(&self) -> bool {
        self.0
            .compare_exchange(QUEUED, STARTED, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }
    fn cancel_before_start(&self) -> bool {
        matches!(
            self.0
                .compare_exchange(QUEUED, CANCELLED, Ordering::AcqRel, Ordering::Acquire),
            Ok(_) | Err(CANCELLED)
        )
    }
}

struct Frame {
    line: Vec<u8>,
    ticket: Arc<Ticket>,
    deadline: Instant,
    stop: Option<Arc<AtomicBool>>,
    reply: mpsc::Sender<WriteOutcome>,
}

enum WriteOutcome {
    Sent,
    Skipped(FailureKind),
    Failed(io::Error),
}

impl Frame {
    fn admit(&self) -> Result<(), FailureKind> {
        let rejection = if self
            .stop
            .as_ref()
            .is_some_and(|stop| stop.load(Ordering::Acquire))
        {
            Some(FailureKind::Cancelled)
        } else if Instant::now() >= self.deadline {
            Some(FailureKind::Timeout)
        } else {
            None
        };
        if let Some(kind) = rejection {
            self.ticket.cancel_before_start();
            return Err(kind);
        }
        if self.ticket.begin() {
            Ok(())
        } else {
            Err(FailureKind::Cancelled)
        }
    }
}

struct Attempt {
    ticket: Arc<Ticket>,
    child: Arc<ServerProcess>,
    done: bool,
}
impl Attempt {
    fn fail(&mut self, kind: FailureKind, message: impl ToString) -> Failure {
        let started = !self.ticket.cancel_before_start();
        if started || kind == FailureKind::Transport {
            self.child.stop();
        }
        self.done = true;
        Failure {
            kind,
            started,
            message: message.to_string(),
        }
    }
}
impl Drop for Attempt {
    fn drop(&mut self) {
        if !self.done && !self.ticket.cancel_before_start() {
            self.child.stop();
        }
    }
}

pub(super) struct Writer {
    sender: Option<SyncSender<Frame>>,
    worker: parking_lot::Mutex<Option<JoinHandle<()>>>,
    child: Arc<ServerProcess>,
    alive: Arc<AtomicBool>,
    #[cfg(test)]
    active: Arc<AtomicBool>,
}
impl Writer {
    pub(super) fn start(
        input: Input,
        child: Arc<ServerProcess>,
        alive: Arc<AtomicBool>,
    ) -> io::Result<Self> {
        let (sender, receiver) = mpsc::sync_channel(8);
        let active = Arc::new(AtomicBool::new(false));
        let process = child.clone();
        let running = alive.clone();
        let writing = active.clone();
        let worker = std::thread::Builder::new()
            .name("sgt-mcp-writer".into())
            .spawn(
                crate::overlay::computer_control::session_retention::inherit(move || {
                    write_frames(input, receiver, &process, &running, &writing);
                }),
            )?;
        Ok(Self {
            sender: Some(sender),
            worker: parking_lot::Mutex::new(Some(worker)),
            child,
            alive,
            #[cfg(test)]
            active,
        })
    }

    pub(super) fn send(
        &self,
        line: Vec<u8>,
        deadline: Instant,
        stop: Option<&AtomicBool>,
        owner: Option<Arc<AtomicBool>>,
    ) -> Result<(), Failure> {
        let ticket = Arc::new(Ticket::default());
        let (reply, receive) = mpsc::channel();
        let mut frame = Frame {
            line,
            ticket: ticket.clone(),
            deadline,
            stop: owner,
            reply,
        };
        let mut attempt = Attempt {
            ticket,
            child: self.child.clone(),
            done: false,
        };
        loop {
            if let Some(kind) = self.interrupted(deadline, stop) {
                return Err(attempt.fail(kind, "MCP frame was not written"));
            }
            match self
                .sender
                .as_ref()
                .expect("writer sender exists")
                .try_send(frame)
            {
                Ok(()) => break,
                Err(TrySendError::Full(returned)) => {
                    frame = returned;
                    std::thread::sleep(
                        POLL.min(deadline.saturating_duration_since(Instant::now())),
                    );
                }
                Err(TrySendError::Disconnected(_)) => {
                    return Err(attempt.fail(FailureKind::Transport, "MCP writer disconnected"));
                }
            }
        }
        loop {
            // A completed frame can use ordinary RPC cancellation rather than
            // retiring a healthy stream merely because its write ack raced stop.
            let ready = receive.try_recv();
            match ready {
                Ok(WriteOutcome::Sent) => {
                    attempt.done = true;
                    return Ok(());
                }
                Ok(WriteOutcome::Skipped(kind)) => {
                    return Err(attempt.fail(kind, "MCP frame admission rejected"));
                }
                Ok(WriteOutcome::Failed(error)) => {
                    return Err(attempt.fail(FailureKind::Transport, error));
                }
                Err(TryRecvError::Disconnected) => {
                    return Err(attempt.fail(FailureKind::Transport, "MCP writer disconnected"));
                }
                Err(TryRecvError::Empty) => {}
            }
            if let Some(kind) = self.interrupted(deadline, stop) {
                return Err(attempt.fail(kind, "MCP frame write was interrupted"));
            }
            match receive.recv_timeout(POLL.min(deadline.saturating_duration_since(Instant::now())))
            {
                Ok(WriteOutcome::Sent) => {
                    attempt.done = true;
                    return Ok(());
                }
                Ok(WriteOutcome::Skipped(kind)) => {
                    return Err(attempt.fail(kind, "MCP frame admission rejected"));
                }
                Ok(WriteOutcome::Failed(error)) => {
                    return Err(attempt.fail(FailureKind::Transport, error));
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(attempt.fail(FailureKind::Transport, "MCP writer disconnected"));
                }
            }
        }
    }

    fn interrupted(&self, deadline: Instant, stop: Option<&AtomicBool>) -> Option<FailureKind> {
        if stop.is_some_and(|stop| stop.load(Ordering::Acquire)) {
            Some(FailureKind::Cancelled)
        } else if Instant::now() >= deadline {
            Some(FailureKind::Timeout)
        } else if !self.alive.load(Ordering::Acquire) {
            Some(FailureKind::Transport)
        } else {
            None
        }
    }

    pub(super) fn join_finished(&self) {
        let mut worker = self.worker.lock();
        if worker.as_ref().is_some_and(JoinHandle::is_finished)
            && let Some(worker) = worker.take()
        {
            let _ = worker.join();
        }
    }

    #[cfg(test)]
    pub(super) fn is_active(&self) -> bool {
        self.active.load(Ordering::Acquire)
    }

    #[cfg(test)]
    pub(super) fn is_finished(&self) -> bool {
        self.worker
            .lock()
            .as_ref()
            .is_none_or(JoinHandle::is_finished)
    }
}
impl Drop for Writer {
    fn drop(&mut self) {
        self.sender.take();
        self.join_finished();
    }
}

fn write_frames(
    mut input: Input,
    receiver: Receiver<Frame>,
    child: &ServerProcess,
    alive: &AtomicBool,
    active: &AtomicBool,
) {
    while alive.load(Ordering::Acquire) {
        let frame = match receiver.recv_timeout(POLL) {
            Ok(frame) => frame,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };
        if !alive.load(Ordering::Acquire) {
            break;
        }
        if let Err(kind) = frame.admit() {
            let _ = frame.reply.send(WriteOutcome::Skipped(kind));
            continue;
        }
        active.store(true, Ordering::Release);
        let result = input.write_all(&frame.line).and_then(|()| input.flush());
        active.store(false, Ordering::Release);
        let failed = result.is_err();
        if failed {
            child.stop();
        }
        let outcome = match result {
            Ok(()) => WriteOutcome::Sent,
            Err(error) => WriteOutcome::Failed(error),
        };
        let _ = frame.reply.send(outcome);
        if failed {
            break;
        }
    }
}

#[derive(Debug)]
pub(super) enum EncodingFailure {
    Cancelled,
    Timeout,
    Invalid(serde_json::Error),
}

pub(super) fn encode(
    message: &Value,
    deadline: Instant,
    stop: Option<&AtomicBool>,
) -> Result<Vec<u8>, EncodingFailure> {
    struct Buffer<'a> {
        bytes: Vec<u8>,
        deadline: Instant,
        stop: Option<&'a AtomicBool>,
        interrupted: Option<FailureKind>,
    }
    impl Write for Buffer<'_> {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.stop.is_some_and(|stop| stop.load(Ordering::Acquire)) {
                self.interrupted = Some(FailureKind::Cancelled);
                return Err(io::Error::other("MCP preparation cancelled"));
            }
            if Instant::now() >= self.deadline {
                self.interrupted = Some(FailureKind::Timeout);
                return Err(io::Error::other("MCP preparation deadline elapsed"));
            }
            if self.bytes.len().saturating_add(bytes.len())
                >= super::super::client_protocol::MAX_JSON_RPC_LINE_BYTES
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "MCP JSON-RPC line exceeds the bounded transport limit",
                ));
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut buffer = Buffer {
        bytes: Vec::new(),
        deadline,
        stop,
        interrupted: None,
    };
    let result = serde_json::to_writer(&mut buffer, message);
    match buffer.interrupted {
        Some(FailureKind::Cancelled) => return Err(EncodingFailure::Cancelled),
        Some(FailureKind::Timeout) => return Err(EncodingFailure::Timeout),
        _ => result.map_err(EncodingFailure::Invalid)?,
    }
    buffer.bytes.push(b'\n');
    Ok(buffer.bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn queued_frame(deadline: Instant, stop: Option<Arc<AtomicBool>>) -> Frame {
        let (reply, _) = mpsc::channel();
        Frame {
            line: b"{}\n".to_vec(),
            ticket: Arc::new(Ticket::default()),
            deadline,
            stop,
            reply,
        }
    }

    #[test]
    fn worker_rejects_cancelled_owner_without_waiting_for_caller() {
        let stop = Arc::new(AtomicBool::new(false));
        let frame = queued_frame(Instant::now() + Duration::from_secs(1), Some(stop.clone()));
        stop.store(true, Ordering::Release);
        assert_eq!(frame.admit(), Err(FailureKind::Cancelled));
        assert!(frame.ticket.cancel_before_start());
        stop.store(false, Ordering::Release);
        assert_eq!(frame.admit(), Err(FailureKind::Cancelled));
        assert!(!frame.ticket.begin());
    }

    #[test]
    fn worker_rejects_expired_deadline_without_waiting_for_caller() {
        let frame = queued_frame(Instant::now(), None);
        assert_eq!(frame.admit(), Err(FailureKind::Timeout));
        assert!(frame.ticket.cancel_before_start());
        assert!(!frame.ticket.begin());
    }

    #[test]
    fn worker_admission_preserves_caller_cancellation_and_claims_only_once() {
        let frame = queued_frame(Instant::now() + Duration::from_secs(1), None);
        assert!(frame.ticket.cancel_before_start());
        assert_eq!(frame.admit(), Err(FailureKind::Cancelled));
        let ready = queued_frame(Instant::now() + Duration::from_secs(1), None);
        assert_eq!(ready.admit(), Ok(()));
        assert!(!ready.ticket.cancel_before_start());
        assert_eq!(ready.admit(), Err(FailureKind::Cancelled));
    }

    #[test]
    fn queued_cancellation_is_monotonic_and_never_claimed_for_write() {
        let ticket = Ticket::default();
        assert!(ticket.cancel_before_start());
        assert!(ticket.cancel_before_start());
        assert!(!ticket.begin());
    }
    #[test]
    fn a_claimed_frame_cannot_be_proven_unstarted() {
        let ticket = Ticket::default();
        assert!(ticket.begin());
        assert!(!ticket.begin());
        assert!(!ticket.cancel_before_start());
    }

    #[test]
    fn claiming_and_cancelling_a_frame_have_one_linearized_winner() {
        for _ in 0..64 {
            let ticket = Arc::new(Ticket::default());
            let gate = Arc::new(std::sync::Barrier::new(2));
            let candidate = ticket.clone();
            let ready = gate.clone();
            let worker = std::thread::spawn(move || {
                ready.wait();
                candidate.begin()
            });
            gate.wait();
            let cancelled = ticket.cancel_before_start();
            let started = worker.join().unwrap();
            assert_ne!(cancelled, started);
            if cancelled {
                assert!(!ticket.begin());
            }
        }
    }

    #[test]
    fn outbound_utf8_and_escape_limit_includes_exactly_one_newline() {
        use super::super::super::client_protocol::MAX_JSON_RPC_LINE_BYTES;
        let overhead = serde_json::to_vec(&serde_json::json!({"value":""}))
            .unwrap()
            .len()
            + 1;
        let escaped = 1024;
        let remaining = MAX_JSON_RPC_LINE_BYTES - overhead - 2 * escaped;
        let value = format!(
            "{}{}{}",
            "\\".repeat(escaped),
            "\u{1f30f}".repeat(remaining / 4),
            "a".repeat(remaining % 4)
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        let exact = encode(&serde_json::json!({"value":value}), deadline, None).unwrap();
        assert_eq!(exact.len(), MAX_JSON_RPC_LINE_BYTES);
        assert_eq!(exact.last(), Some(&b'\n'));
        assert_eq!(exact.iter().filter(|byte| **byte == b'\n').count(), 1);
        let oversized = encode(
            &serde_json::json!({"value":format!("{value}x")}),
            deadline,
            None,
        )
        .unwrap_err();
        assert!(
            matches!(oversized, EncodingFailure::Invalid(error) if error.io_error_kind() == Some(io::ErrorKind::InvalidData))
        );
    }

    #[test]
    fn preparation_preserves_typed_cancellation_and_deadline_outcomes() {
        let value = serde_json::json!({"value":"data"});
        let stop = AtomicBool::new(true);
        let cancelled =
            encode(&value, Instant::now() + Duration::from_secs(1), Some(&stop)).unwrap_err();
        assert!(matches!(cancelled, EncodingFailure::Cancelled));
        let elapsed = encode(&value, Instant::now(), None).unwrap_err();
        assert!(matches!(elapsed, EncodingFailure::Timeout));
    }
}
