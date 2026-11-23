//! Public session entry points and the typed-command bridge.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};

use super::super::overlay;

pub(crate) const MAXIMUM_TEXT_COMMAND_CHARS: usize = 1_024;

static TEXT_COMMAND_TX: Mutex<Option<TextCommandBridge>> = Mutex::new(None);
static STARTUP_TEXT_COMMAND: Mutex<Option<TextCommand>> = Mutex::new(None);
static TURN_IDLE: AtomicBool = AtomicBool::new(false);
static TEXT_COMMAND_QUEUED: AtomicBool = AtomicBool::new(false);
// Serialize a stop request with admission of session-owned network effects.
static RECONNECT_DISPATCH: Mutex<()> = Mutex::new(());

pub(crate) fn request_stop(stop: &AtomicBool) {
    let _guard = RECONNECT_DISPATCH
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    stop.store(true, Ordering::SeqCst);
}

pub(crate) fn reconnect_effect<T>(
    stop: &AtomicBool,
    effect: impl FnOnce() -> anyhow::Result<T>,
) -> anyhow::Result<T> {
    reconnect_attempt_effect(stop, None, None, effect)
}

pub(crate) fn reconnect_attempt_effect<T>(
    stop: &AtomicBool,
    attempt_stop: Option<&AtomicBool>,
    deadline: Option<std::time::Instant>,
    effect: impl FnOnce() -> anyhow::Result<T>,
) -> anyhow::Result<T> {
    let _guard = RECONNECT_DISPATCH
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    super::session::check_connection_attempt(stop, attempt_stop, deadline)?;
    effect()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TextCommandDisposition {
    Queued,
    Busy,
    Invalid,
}

pub(super) struct TextCommandSenderGuard;

struct TextCommandBridge {
    sender: mpsc::SyncSender<TextCommand>,
    owner: super::super::session_retention::SessionRetention,
}

pub(super) struct TextCommand {
    text: String,
    nonpersistent: bool,
}

impl TextCommand {
    fn apply_retention(&self) {
        if self.nonpersistent {
            super::super::session_retention::current().forbid();
        }
    }

    pub(super) fn into_text(self) -> String {
        self.apply_retention();
        self.text
    }
}

pub(super) fn install_text_sender(sender: mpsc::SyncSender<TextCommand>) -> TextCommandSenderGuard {
    let pending = STARTUP_TEXT_COMMAND.lock().unwrap().take();
    *TEXT_COMMAND_TX.lock().unwrap() = Some(TextCommandBridge {
        sender: sender.clone(),
        owner: super::super::session_retention::current(),
    });
    if let Some(text) = pending {
        text.apply_retention();
        if sender.try_send(text).is_err() {
            TEXT_COMMAND_QUEUED.store(false, Ordering::SeqCst);
        }
    }
    TextCommandSenderGuard
}

impl Drop for TextCommandSenderGuard {
    fn drop(&mut self) {
        TURN_IDLE.store(false, Ordering::SeqCst);
        TEXT_COMMAND_QUEUED.store(false, Ordering::SeqCst);
        *TEXT_COMMAND_TX.lock().unwrap() = None;
    }
}

pub(crate) fn submit_text_command(text: String) -> TextCommandDisposition {
    enqueue_text_command(text, false)
}

pub(crate) fn submit_external_text_command(text: String) -> TextCommandDisposition {
    enqueue_text_command(text, true)
}

fn enqueue_text_command(text: String, nonpersistent: bool) -> TextCommandDisposition {
    let text = text.trim().to_string();
    if text.is_empty() || text.chars().count() > MAXIMUM_TEXT_COMMAND_CHARS {
        return TextCommandDisposition::Invalid;
    }
    let text = TextCommand {
        text,
        nonpersistent,
    };
    if let Some(bridge) = TEXT_COMMAND_TX.lock().unwrap().as_ref() {
        TEXT_COMMAND_QUEUED.store(true, Ordering::SeqCst);
        return match bridge.sender.try_send(text) {
            Ok(()) => {
                if nonpersistent {
                    bridge.owner.forbid();
                }
                TextCommandDisposition::Queued
            }
            Err(mpsc::TrySendError::Full(_)) => TextCommandDisposition::Busy,
            Err(mpsc::TrySendError::Disconnected(_)) => {
                TEXT_COMMAND_QUEUED.store(false, Ordering::SeqCst);
                TextCommandDisposition::Busy
            }
        };
    }
    let mut pending = STARTUP_TEXT_COMMAND.lock().unwrap();
    if pending.is_some() {
        TextCommandDisposition::Busy
    } else {
        *pending = Some(text);
        TEXT_COMMAND_QUEUED.store(true, Ordering::SeqCst);
        TextCommandDisposition::Queued
    }
}

pub(crate) fn clear_startup_text_command() {
    STARTUP_TEXT_COMMAND.lock().unwrap().take();
    TEXT_COMMAND_QUEUED.store(false, Ordering::SeqCst);
}

pub(super) fn mark_text_command_consumed() {
    TEXT_COMMAND_QUEUED.store(false, Ordering::SeqCst);
}

pub(crate) fn text_command_queued() -> bool {
    TEXT_COMMAND_QUEUED.load(Ordering::SeqCst)
}

pub(super) fn set_turn_idle(idle: bool) {
    TURN_IDLE.store(idle, Ordering::SeqCst);
}

pub(crate) fn turn_idle() -> bool {
    TURN_IDLE.load(Ordering::SeqCst)
}

pub(crate) fn run(stop: Arc<AtomicBool>) {
    match super::run_inner(&stop) {
        Ok(()) => overlay::set_status("stopped"),
        Err(error) => {
            use super::session::SessionFailureKind;
            match super::session::failure_kind(&error) {
                SessionFailureKind::Stopped => overlay::set_status("stopped"),
                SessionFailureKind::Credential => {
                    overlay::show_startup_credential_error(&error);
                    overlay::push_log("Gemini API key was rejected during startup".to_string());
                    overlay::set_status("error");
                }
                SessionFailureKind::ResourceLimit => {
                    overlay::push_log(format!(
                        "Provider reported a resource limit. Check the provider's quota and retry guidance: {error}"
                    ));
                    overlay::set_status("provider resource limit");
                }
                SessionFailureKind::Other => {
                    overlay::push_log(format!("[warn] session error: {error}"));
                    overlay::set_status("error");
                }
            }
        }
    }
    clear_startup_text_command();
    overlay::set_listening(false);
}

#[cfg(test)]
mod tests {
    use super::*;

    static COMMAND_TEST: Mutex<()> = Mutex::new(());

    #[test]
    fn external_command_retains_its_policy_through_startup_and_live_queues() {
        let _serial = COMMAND_TEST.lock().unwrap();
        let owner = super::super::super::session_retention::SessionRetention::default();
        let _scope = owner.enter();
        clear_startup_text_command();
        assert_eq!(
            submit_external_text_command("private startup".into()),
            TextCommandDisposition::Queued
        );
        let (sender, receiver) = mpsc::sync_channel(1);
        let bridge = install_text_sender(sender);
        // Startup frame capture must already be nonpersistent before consumption.
        assert!(!owner.allows());
        assert_eq!(receiver.try_recv().unwrap().into_text(), "private startup");
        assert_eq!(
            submit_external_text_command("private live".into()),
            TextCommandDisposition::Queued
        );
        assert_eq!(receiver.try_recv().unwrap().into_text(), "private live");
        assert_eq!(
            submit_text_command("later local input".into()),
            TextCommandDisposition::Queued
        );
        assert_eq!(
            receiver.try_recv().unwrap().into_text(),
            "later local input"
        );
        assert!(!owner.allows());
        drop(bridge);
        clear_startup_text_command();
        let fresh = super::super::super::session_retention::SessionRetention::default();
        let _fresh_scope = fresh.enter();
        let (sender, receiver) = mpsc::sync_channel(1);
        let _bridge = install_text_sender(sender);
        assert!(fresh.allows());
        assert_eq!(
            submit_external_text_command("private live".into()),
            TextCommandDisposition::Queued
        );
        // Admission also fences background sinks before the consumer dequeues.
        assert!(!fresh.allows());
        assert_eq!(receiver.try_recv().unwrap().into_text(), "private live");
    }

    #[test]
    fn expired_attempt_cannot_dispatch_after_waiting_for_admission_lock() {
        let guard = RECONNECT_DISPATCH
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let stop = Arc::new(AtomicBool::new(false));
        let attempt = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let worker_attempt = Arc::clone(&attempt);
        let (ready_tx, ready_rx) = mpsc::channel();
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(40);
        let worker = std::thread::spawn(move || {
            ready_tx.send(()).unwrap();
            let mut called = false;
            let result = reconnect_attempt_effect(
                &worker_stop,
                Some(&worker_attempt),
                Some(deadline),
                || {
                    called = true;
                    Ok(())
                },
            );
            (result, called)
        });
        ready_rx.recv().unwrap();
        while std::time::Instant::now() < deadline {
            std::thread::yield_now();
        }
        assert!(!attempt.load(Ordering::SeqCst));
        drop(guard);
        let (result, called) = worker.join().unwrap();
        let error = result.unwrap_err();
        assert_eq!(
            super::super::session::failure_kind(&error),
            super::super::session::SessionFailureKind::Other
        );
        assert!(!called);
    }

    #[test]
    fn stop_serializes_with_an_admitted_reconnect_effect() {
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (stopped_tx, stopped_rx) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            reconnect_effect(&worker_stop, || {
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                Ok(())
            })
        });
        entered_rx.recv().unwrap();
        let stopper_stop = Arc::clone(&stop);
        let stopper = std::thread::spawn(move || {
            request_stop(&stopper_stop);
            stopped_tx.send(()).unwrap();
        });
        assert!(
            stopped_rx
                .recv_timeout(std::time::Duration::from_millis(50))
                .is_err()
        );
        release_tx.send(()).unwrap();
        worker.join().unwrap().unwrap();
        stopped_rx
            .recv_timeout(std::time::Duration::from_secs(1))
            .unwrap();
        stopper.join().unwrap();
        let mut called = false;
        let result = reconnect_effect(&stop, || {
            called = true;
            Ok(())
        });
        assert_eq!(
            super::super::session::failure_kind(&result.unwrap_err()),
            super::super::session::SessionFailureKind::Stopped
        );
        assert!(!called);
    }

    #[test]
    fn text_command_queue_is_bounded_and_validated() {
        let _serial = COMMAND_TEST.lock().unwrap();
        clear_startup_text_command();
        assert_eq!(
            submit_text_command("  first  ".into()),
            TextCommandDisposition::Queued
        );
        assert!(text_command_queued());
        assert_eq!(
            submit_text_command("second".into()),
            TextCommandDisposition::Busy
        );
        clear_startup_text_command();
        assert!(!text_command_queued());
        assert_eq!(
            submit_text_command("   ".into()),
            TextCommandDisposition::Invalid
        );
        assert_eq!(
            submit_text_command("x".repeat(MAXIMUM_TEXT_COMMAND_CHARS + 1)),
            TextCommandDisposition::Invalid
        );
    }

    #[test]
    fn prompt_limit_is_the_shared_fixture_count_of_unicode_scalars() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../parity-fixtures/phone-control/external-control.json"
        ))
        .unwrap();
        let limit = fixture["maximumPromptCharacters"].as_u64().unwrap() as usize;
        assert_eq!(MAXIMUM_TEXT_COMMAND_CHARS, limit);
        let _serial = COMMAND_TEST.lock().unwrap();
        clear_startup_text_command();
        // Each emoji is one scalar but two UTF-16 units: the rule counts scalars.
        assert_eq!(
            submit_text_command("\u{1F600}".repeat(limit)),
            TextCommandDisposition::Queued
        );
        clear_startup_text_command();
        assert_eq!(
            submit_text_command("\u{1F600}".repeat(limit + 1)),
            TextCommandDisposition::Invalid
        );
        clear_startup_text_command();
    }
}
