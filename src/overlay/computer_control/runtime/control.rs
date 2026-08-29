//! Public session entry points and the typed-command bridge.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};

use super::super::overlay;

pub(crate) const MAXIMUM_TEXT_COMMAND_CHARS: usize = 1_024;

static TEXT_COMMAND_TX: Mutex<Option<mpsc::SyncSender<String>>> = Mutex::new(None);
static STARTUP_TEXT_COMMAND: Mutex<Option<String>> = Mutex::new(None);
static TURN_IDLE: AtomicBool = AtomicBool::new(false);
static TEXT_COMMAND_QUEUED: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TextCommandDisposition {
    Queued,
    Busy,
    Invalid,
}

pub(super) struct TextCommandSenderGuard;

pub(super) fn install_text_sender(sender: mpsc::SyncSender<String>) -> TextCommandSenderGuard {
    let pending = STARTUP_TEXT_COMMAND.lock().unwrap().take();
    *TEXT_COMMAND_TX.lock().unwrap() = Some(sender.clone());
    if let Some(text) = pending
        && sender.try_send(text).is_err()
    {
        TEXT_COMMAND_QUEUED.store(false, Ordering::SeqCst);
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
    let text = text.trim().to_string();
    if text.is_empty() || text.chars().count() > MAXIMUM_TEXT_COMMAND_CHARS {
        return TextCommandDisposition::Invalid;
    }
    if let Some(sender) = TEXT_COMMAND_TX.lock().unwrap().as_ref() {
        TEXT_COMMAND_QUEUED.store(true, Ordering::SeqCst);
        return match sender.try_send(text) {
            Ok(()) => TextCommandDisposition::Queued,
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
    run_with_turns(stop, None);
}

fn run_with_turns(stop: Arc<AtomicBool>, turns: Option<Vec<String>>) {
    match super::run_inner(&stop, turns) {
        Ok(()) => overlay::set_status("stopped"),
        Err(error) => {
            let message = error.to_string().to_lowercase();
            // Cleanup sets the shared stop flag on every exit. Only the explicit
            // cancellation sentinel is therefore evidence of a normal stop;
            // transport/setup failures must remain visible.
            if message == "stopped" {
                overlay::set_status("stopped");
            } else if overlay::show_startup_credential_error(&error) {
                overlay::push_log("Gemini API key was rejected during startup".to_string());
                overlay::set_status("error");
            } else if message.contains("quota")
                || message.contains("exceeded")
                || message.contains("resource_exhausted")
            {
                overlay::push_log(
                    "Gemini rate limit hit (a burst of Live connections). This is usually the per-minute / \
concurrent-session cap, NOT your daily quota - just WAIT ~30-60s and start again. If it persists, check the key \
matches your AI Studio project, or use a billing-enabled key."
                        .to_string(),
                );
                overlay::set_status("rate limited - wait ~1 min and retry");
            } else {
                overlay::push_log(format!("[warn] session error: {error}"));
                overlay::set_status("error");
            }
        }
    }
    clear_startup_text_command();
    overlay::set_listening(false);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_command_queue_is_bounded_and_validated() {
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
}
