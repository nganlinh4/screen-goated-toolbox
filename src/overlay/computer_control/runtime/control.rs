//! Public session entry points and the typed-command bridge.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, mpsc};

use super::super::overlay;

static TEXT_COMMAND_TX: Mutex<Option<mpsc::Sender<String>>> = Mutex::new(None);

pub(super) fn install_text_sender(sender: mpsc::Sender<String>) {
    *TEXT_COMMAND_TX.lock().unwrap() = Some(sender);
}

pub(crate) fn submit_text_command(text: String) {
    if let Ok(guard) = TEXT_COMMAND_TX.lock()
        && let Some(sender) = guard.as_ref()
    {
        let _ = sender.send(text);
    }
}

pub(crate) fn run(stop: Arc<AtomicBool>) {
    run_with_turns(stop, None);
}

pub(crate) fn run_scripted(stop: Arc<AtomicBool>, turns: Vec<String>) -> anyhow::Result<()> {
    let result = super::run_inner(&stop, Some(turns));
    overlay::set_listening(false);
    result
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
    overlay::set_listening(false);
}
