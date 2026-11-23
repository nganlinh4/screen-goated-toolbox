//! Lifecycle tests for `WorkerSession` against a scripted fake worker.
//!
//! The session owns a real contained child process (an idle PowerShell), so
//! termination and exit detection are the production paths. Its request
//! stream goes to a responder thread that decodes each authenticated request
//! and answers on the session's response channel the way the worker's
//! response reader would.

use super::*;
use sgt_recorder_protocol::{PROTOCOL_VERSION, Request};
use std::io::Write;

/// What the fake worker does with one request.
enum Reply {
    Result(serde_json::Value),
    Error(&'static str),
    Failed(&'static str),
    /// A late answer to an earlier request, then the real result.
    StaleThenResult(serde_json::Value),
    /// No answer: a hung worker.
    Silent,
    /// The worker finalizes and exits without answering.
    Exit,
}

/// Request stream writer that hands each complete line to the responder.
struct LineSink {
    pending: Vec<u8>,
    lines: mpsc::Sender<Vec<u8>>,
}

impl Write for LineSink {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.pending.extend_from_slice(bytes);
        while let Some(end) = self.pending.iter().position(|byte| *byte == b'\n') {
            let line: Vec<u8> = self.pending.drain(..=end).collect();
            let _ = self.lines.send(line);
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct FakeWorker {
    session: WorkerSession,
    received: Arc<Mutex<Vec<Command>>>,
}

fn powershell(script: &str) -> ProcessCommand {
    let path = crate::system_command::system_relative_executable(&[
        "WindowsPowerShell",
        "v1.0",
        "powershell.exe",
    ])
    .unwrap();
    let mut command = crate::system_command::background_command(path);
    command.args([
        "-NoLogo",
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        script,
    ]);
    command
}

fn response(token: &str, request_id: u64, result: Option<serde_json::Value>) -> ResponseEvent {
    ResponseEvent::Response(Response {
        protocol_version: PROTOCOL_VERSION,
        token: token.to_string(),
        request_id,
        result,
        error: None,
    })
}

/// Starts a fake worker that answers the n-th request with `replies[n]`.
fn fake_worker(script: &str, replies: Vec<Reply>) -> FakeWorker {
    let token = launch_token().unwrap();
    let child = spawn_worker_process(&mut powershell(script), ()).unwrap();
    // Workers start suspended inside their job; launch resumes them.
    child.resume().unwrap();
    let group = child.group();
    let (line_tx, line_rx) = mpsc::channel::<Vec<u8>>();
    let (response_tx, response_rx) = mpsc::channel();
    let received = Arc::new(Mutex::new(Vec::new()));
    let log = received.clone();
    let reply_token = token.clone();
    std::thread::spawn(move || {
        let mut replies = replies.into_iter();
        for line in line_rx {
            let request: Request = serde_json::from_slice(&line).unwrap();
            log.lock().unwrap().push(request.command.clone());
            let id = request.request_id;
            let events = match replies.next() {
                Some(Reply::Result(value)) => vec![response(&reply_token, id, Some(value))],
                Some(Reply::Error(message)) => {
                    let ResponseEvent::Response(mut answer) = response(&reply_token, id, None)
                    else {
                        unreachable!()
                    };
                    answer.error = Some(message.to_string());
                    vec![ResponseEvent::Response(answer)]
                }
                Some(Reply::Failed(reason)) => vec![ResponseEvent::Failed(reason.to_string())],
                Some(Reply::StaleThenResult(value)) => vec![
                    response(&reply_token, id - 1, Some(serde_json::json!("stale"))),
                    response(&reply_token, id, Some(value)),
                ],
                Some(Reply::Exit) => {
                    let _ = group.terminate();
                    Vec::new()
                }
                Some(Reply::Silent) | None => Vec::new(),
            };
            for event in events {
                let _ = response_tx.send(event);
            }
        }
    });
    let session = WorkerSession {
        child,
        stdin: Arc::new(SessionWriter::new(
            token.clone(),
            LineSink {
                pending: Vec::new(),
                lines: line_tx,
            },
        )),
        responses: response_rx,
        token,
        progress: progress::ProgressSession::new(),
    };
    FakeWorker { session, received }
}

const IDLE: &str = "Start-Sleep -Seconds 60";
const SHORT: Duration = Duration::from_secs(5);

fn stops_within(session: &mut WorkerSession, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if session.child.try_wait().ok().flatten().is_some() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

#[test]
fn a_command_error_keeps_a_disposable_worker_for_its_graceful_shutdown() {
    let mut worker = fake_worker(
        IDLE,
        vec![
            Reply::Error("decode failed: unsupported codec"),
            Reply::Result(serde_json::json!({})),
        ],
    );
    let error = worker.session.request(Command::Ping, SHORT).unwrap_err();
    assert_eq!(error.to_string(), "decode failed: unsupported codec");
    assert!(
        !stops_within(&mut worker.session, Duration::from_millis(500)),
        "a business error must not kill the worker"
    );

    worker.session.shutdown().unwrap();
    assert!(worker.session.child.try_wait().unwrap().is_some());
    let received = worker.received.lock().unwrap();
    assert!(matches!(
        received.as_slice(),
        [Command::Ping, Command::Shutdown]
    ));
}

#[test]
fn a_transport_failure_retires_a_disposable_worker() {
    let mut worker = fake_worker(IDLE, vec![Reply::Failed("invalid frame")]);
    let error = worker.session.request(Command::Ping, SHORT).unwrap_err();
    assert!(
        error.to_string().contains("control connection failed"),
        "{error}"
    );
    assert!(stops_within(&mut worker.session, SHORT));
}

#[test]
fn the_interactive_window_survives_both_kinds_of_error() {
    let mut worker = fake_worker(
        IDLE,
        vec![
            Reply::Error("no active recording"),
            Reply::Failed("invalid frame"),
            Reply::Result(serde_json::json!({"ok": true})),
        ],
    );
    assert!(
        worker
            .session
            .request_preserving(Command::Ping, SHORT)
            .is_err()
    );
    assert!(
        worker
            .session
            .request_preserving(Command::Ping, SHORT)
            .is_err()
    );
    assert!(!stops_within(
        &mut worker.session,
        Duration::from_millis(300)
    ));
    let value = worker
        .session
        .request_preserving(Command::Ping, SHORT)
        .unwrap();
    assert_eq!(value, serde_json::json!({"ok": true}));
    worker.session.child.stop_within(SHORT).unwrap();
}

#[test]
fn a_late_answer_to_an_earlier_request_is_skipped() {
    let mut worker = fake_worker(
        IDLE,
        vec![
            Reply::Result(serde_json::json!("first")),
            Reply::StaleThenResult(serde_json::json!("current")),
        ],
    );
    let first = worker.session.request(Command::Ping, SHORT).unwrap();
    assert_eq!(first, serde_json::json!("first"));
    let value = worker.session.request(Command::Ping, SHORT).unwrap();
    assert_eq!(value, serde_json::json!("current"));
    worker.session.child.stop_within(SHORT).unwrap();
}

#[test]
fn an_exited_worker_is_dropped_so_the_next_command_relaunches() {
    let mut live = Some(fake_worker(IDLE, Vec::new()).session);
    request::drop_exited_session(&mut live);
    assert!(live.is_some(), "a running worker is kept");
    live.as_mut().unwrap().child.stop_within(SHORT).unwrap();

    let mut exited = Some(fake_worker("exit 3", Vec::new()).session);
    assert!(stops_within(
        exited.as_mut().unwrap(),
        Duration::from_secs(30)
    ));
    request::drop_exited_session(&mut exited);
    assert!(
        exited.is_none(),
        "an exited worker is replaced on the next command"
    );

    // A request on a session whose worker already exited reports it instead
    // of writing into a dead pipe.
    let mut worker = fake_worker("exit 0", Vec::new());
    assert!(stops_within(&mut worker.session, Duration::from_secs(30)));
    let error = worker.session.request(Command::Ping, SHORT).unwrap_err();
    assert!(error.to_string().contains("exited"), "{error}");
    assert!(worker.received.lock().unwrap().is_empty());
}

fn job_ends_within(job: &crate::process_job::JobControl, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if job.active_processes().is_ok_and(|count| count == 0) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

fn interactive_request(
    session: &mut Option<WorkerSession>,
    timeout: Duration,
) -> Result<serde_json::Value> {
    request_with_capability_retry(
        session,
        Command::Ping,
        timeout,
        session_lifecycle::JobSlot::Interactive,
        ComponentDelivery::InstallIfMissing,
    )
}

#[test]
fn a_hung_window_is_asked_to_finalize_then_retired_so_the_next_command_relaunches() {
    let worker = fake_worker(
        IDLE,
        vec![
            Reply::Silent,
            Reply::Result(serde_json::json!({"status": "ready"})),
            Reply::Exit,
        ],
    );
    let job = worker.session.child.control();
    let mut session = Some(worker.session);
    let started = Instant::now();
    let error = interactive_request(&mut session, Duration::from_millis(300)).unwrap_err();
    assert!(error.to_string().contains("timed out"), "{error}");
    assert!(
        session.is_none(),
        "a window that stopped answering is retired, so the next command relaunches it"
    );
    assert!(
        started.elapsed() < request::UNRESPONSIVE_STOP_GRACE,
        "a window that finalizes and exits is not held to the kill deadline"
    );
    assert!(matches!(
        worker.received.lock().unwrap().as_slice(),
        [Command::Ping, Command::PrepareShutdown, Command::Shutdown]
    ));
    assert!(job_ends_within(&job, SHORT));
}

#[test]
fn a_window_that_ignores_the_graceful_stop_is_killed_at_the_deadline() {
    let worker = fake_worker(IDLE, Vec::new());
    let job = worker.session.child.control();
    let mut session = Some(worker.session);
    let grace = Duration::from_millis(400);
    let started = Instant::now();
    request::retire_unresponsive(&mut session, grace);
    assert!(
        started.elapsed() >= grace,
        "the graceful stop gets its full grace"
    );
    assert!(started.elapsed() < grace + SHORT);
    assert!(session.is_none());
    assert!(job_ends_within(&job, SHORT), "the hung worker is killed");
    assert!(matches!(
        worker.received.lock().unwrap().as_slice(),
        [Command::PrepareShutdown, Command::Shutdown]
    ));
}

#[test]
fn a_command_error_keeps_the_window_but_a_broken_connection_retires_it() {
    let worker = fake_worker(
        IDLE,
        vec![
            Reply::Error("no active recording"),
            Reply::Failed("invalid frame"),
            Reply::Result(serde_json::json!({"status": "ready"})),
            Reply::Exit,
        ],
    );
    let job = worker.session.child.control();
    let mut session = Some(worker.session);
    let error = interactive_request(&mut session, SHORT).unwrap_err();
    assert_eq!(error.to_string(), "no active recording");
    assert!(session.is_some(), "a command error keeps the window");
    assert!(!stops_within(
        session.as_mut().unwrap(),
        Duration::from_millis(300)
    ));

    let error = interactive_request(&mut session, SHORT).unwrap_err();
    assert!(
        error.to_string().contains("control connection failed"),
        "{error}"
    );
    assert!(session.is_none(), "a broken connection retires the window");
    assert!(job_ends_within(&job, SHORT));
    assert!(matches!(
        worker.received.lock().unwrap().as_slice(),
        [
            Command::Ping,
            Command::Ping,
            Command::PrepareShutdown,
            Command::Shutdown
        ]
    ));
}
