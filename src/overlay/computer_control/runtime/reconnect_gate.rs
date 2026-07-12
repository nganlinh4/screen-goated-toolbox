//! Speech-safe gates for intentional Live-session replacement.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::super::{mcp, overlay, telemetry};
use super::mic;

pub(super) fn user_audio_active(buf: &Arc<Mutex<Vec<i16>>>, last_voice: Instant) -> bool {
    last_voice.elapsed() < Duration::from_secs(2)
        || buf.lock().is_ok_and(|samples| mic::is_voiced(&samples))
}

pub(super) fn record_catalog_deferral(active: bool, already_logged: &mut bool) {
    if active && !*already_logged && mcp::tools_changed() {
        *already_logged = true;
        overlay::push_log("(mcp) reconnect deferred until user finishes speaking".to_string());
        telemetry::event(
            "session_reconnect_deferred",
            "speech",
            telemetry::Privacy::Safe,
            serde_json::json!({"trigger": "tool_catalog_changed", "reason": "user_audio_active"}),
        );
    } else if !active {
        *already_logged = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_or_buffered_voice_blocks_a_session_swap() {
        let quiet = Arc::new(Mutex::new(Vec::new()));
        assert!(user_audio_active(&quiet, Instant::now()));
        let old = Instant::now() - Duration::from_secs(3);
        assert!(!user_audio_active(&quiet, old));

        let voiced = Arc::new(Mutex::new(vec![300, -300]));
        assert!(user_audio_active(&voiced, old));
    }
}
