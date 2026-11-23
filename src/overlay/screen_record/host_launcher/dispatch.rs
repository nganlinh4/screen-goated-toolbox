//! Command dispatch for the recorder worker processes.
//!
//! Interactive commands (Show, Toggle, settings, shutdown) run on one thread
//! against the long-lived recorder window. Headless jobs run on their own
//! thread against their own short-lived worker, so a five-minute audio decode
//! never delays a hotkey Toggle and never collides with the open window.

use super::*;
use session_lifecycle::JobSlot;

pub(super) enum DispatchMessage {
    Interactive(Command),
    Stop(Arc<AtomicBool>, mpsc::SyncSender<Result<(), String>>),
    Exit(Arc<AtomicBool>, mpsc::SyncSender<Result<(), String>>),
}

pub(super) enum HeadlessMessage {
    Run(Command, mpsc::SyncSender<Result<serde_json::Value, String>>),
    /// Answered once every job queued before it has finished.
    Drain(mpsc::SyncSender<()>),
}

pub(super) static HEADLESS_DISPATCHER: LazyLock<mpsc::Sender<HeadlessMessage>> =
    LazyLock::new(|| spawn_headless_queue(dispatch_headless));

/// Starts the serial headless job queue on its own thread.
pub(super) fn spawn_headless_queue(
    run: impl FnMut(Command) -> Result<serde_json::Value> + Send + 'static,
) -> mpsc::Sender<HeadlessMessage> {
    let (sender, receiver) = mpsc::channel();
    std::thread::Builder::new()
        .name("sgt-recorder-headless".to_string())
        .spawn(move || headless_loop(receiver, run))
        .expect("spawn recorder headless dispatcher");
    sender
}

fn headless_loop(
    receiver: mpsc::Receiver<HeadlessMessage>,
    mut run: impl FnMut(Command) -> Result<serde_json::Value>,
) {
    for message in receiver {
        match message {
            HeadlessMessage::Run(command, sender) => {
                let result = run(command).map_err(|error| format!("{error:#}"));
                let _ = sender.send(result);
            }
            HeadlessMessage::Drain(sender) => {
                let _ = sender.send(());
            }
        }
    }
}

/// Waits until every headless job queued so far has finished.
pub(super) fn drain_headless(
    queue: &mpsc::Sender<HeadlessMessage>,
    timeout: Duration,
) -> Result<()> {
    let (sender, receiver) = mpsc::sync_channel(1);
    queue
        .send(HeadlessMessage::Drain(sender))
        .map_err(|_| anyhow!("recorder headless dispatcher is unavailable"))?;
    receiver
        .recv_timeout(timeout)
        .context("recorder headless job did not stop")
}

pub(super) fn dispatch_loop(receiver: mpsc::Receiver<DispatchMessage>) {
    let mut interactive: Option<WorkerSession> = None;
    for message in receiver {
        match message {
            DispatchMessage::Interactive(command) => {
                let shutdown = matches!(command, Command::Shutdown);
                let result = dispatch_interactive(&mut interactive, command);
                let exited = interactive
                    .as_mut()
                    .is_some_and(|session| matches!(session.child.try_wait(), Ok(Some(_))));
                if shutdown && result.is_ok() || exited {
                    interactive.take();
                }
                // A failed launch or a retired window leaves nothing running;
                // the next Show or Toggle launches a fresh one.
                ACTIVE.store(interactive.is_some(), Ordering::Release);
                if let Err(error) = result {
                    report_launch_error(&error);
                }
            }
            DispatchMessage::Stop(cancelled, sender) => {
                let result = if cancelled.load(Ordering::Acquire) {
                    Err(anyhow!("recorder removal was canceled"))
                } else if let Some(session) = interactive.as_mut() {
                    session
                        .request_preserving(Command::PrepareShutdown, PREPARE_TIMEOUT)
                        .and_then(|_| {
                            if cancelled.load(Ordering::Acquire) {
                                let _ = session
                                    .request_preserving(Command::CancelShutdown, NORMAL_TIMEOUT);
                                bail!("recorder removal was canceled");
                            }
                            session.request(Command::Shutdown, NORMAL_TIMEOUT)
                        })
                        .map(|_| ())
                } else {
                    Ok(())
                };
                if result.is_ok() {
                    interactive.take();
                    ACTIVE.store(false, Ordering::Release);
                }
                let _ = sender.send(result.map_err(|error| format!("{error:#}")));
            }
            DispatchMessage::Exit(cancelled, sender) => {
                let result = if cancelled.load(Ordering::Acquire) {
                    Err(anyhow!("recorder shutdown was canceled"))
                } else if let Some(session) = interactive.as_mut() {
                    session
                        .request_preserving(Command::PrepareShutdown, PREPARE_TIMEOUT)
                        .and_then(|_| {
                            if cancelled.load(Ordering::Acquire) {
                                let _ = session
                                    .request_preserving(Command::CancelShutdown, NORMAL_TIMEOUT);
                                bail!("recorder shutdown was canceled");
                            }
                            session.shutdown()?;
                            Ok(())
                        })
                } else {
                    Ok(())
                };
                if result.is_ok() {
                    interactive.take();
                    ACTIVE.store(false, Ordering::Release);
                }
                let _ = sender.send(result.map_err(|error| format!("{error:#}")));
            }
        }
    }
}

fn dispatch_interactive(session: &mut Option<WorkerSession>, command: Command) -> Result<()> {
    for pending in super::super::pending_commands() {
        request_with_capability_retry(
            session,
            pending,
            NORMAL_TIMEOUT,
            JobSlot::Interactive,
            ComponentDelivery::InstallIfMissing,
        )?;
    }
    request_with_capability_retry(
        session,
        command,
        NORMAL_TIMEOUT,
        JobSlot::Interactive,
        ComponentDelivery::InstallIfMissing,
    )?;
    Ok(())
}

fn dispatch_headless(command: Command) -> Result<serde_json::Value> {
    let mut session = None;
    let timeout = match &command {
        Command::DecodeAudio { .. } => CODEC_TIMEOUT,
        _ => HEADLESS_TIMEOUT,
    };
    let delivery = ComponentDelivery::for_headless(&command);
    let result =
        request_with_capability_retry(&mut session, command, timeout, JobSlot::Headless, delivery);
    if let Err(error) = &result
        && error
            .downcast_ref::<OptionalComponentUnavailable>()
            .is_some()
    {
        crate::log_info!("[ScreenRecord] {error}");
    }
    if let Some(session) = session.as_mut() {
        let _ = session.shutdown();
    }
    result
}

fn report_launch_error(error: &anyhow::Error) {
    crate::log_info!("[ScreenRecord] worker failed: {error:#}");
    crate::overlay::auto_copy_badge::show_detailed_notification(
        "Screen Recorder unavailable",
        &format!("{error:#}"),
        crate::overlay::auto_copy_badge::NotificationType::Error,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headless_jobs_run_off_the_caller_and_drain_waits_for_them() {
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel::<()>();
        let queue = spawn_headless_queue(move |command| {
            let _ = started_tx.send(std::thread::current().name().map(str::to_string));
            release_rx.recv().ok();
            Ok(serde_json::json!({ "ping": matches!(command, Command::Ping) }))
        });
        let (result_tx, result_rx) = mpsc::sync_channel(1);
        queue
            .send(HeadlessMessage::Run(Command::Ping, result_tx))
            .unwrap();
        let thread = started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(thread.as_deref(), Some("sgt-recorder-headless"));

        // While the long job runs, a drain must keep waiting rather than
        // report the queue idle.
        assert!(drain_headless(&queue, Duration::from_millis(100)).is_err());
        release_tx.send(()).unwrap();
        assert_eq!(
            result_rx.recv_timeout(Duration::from_secs(5)).unwrap(),
            Ok(serde_json::json!({ "ping": true }))
        );
        drain_headless(&queue, Duration::from_secs(5)).unwrap();
    }
}
