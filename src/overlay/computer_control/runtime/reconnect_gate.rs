//! Speech-safe gates for intentional Live-session replacement.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::super::protocol::ServerEvent;
use super::super::{mcp, overlay, telemetry};
use super::mic;
use super::reader::Reader;

pub(super) fn generation_progress(event: &ServerEvent) -> bool {
    match event {
        ServerEvent::Audio(samples) => !samples.is_empty(),
        ServerEvent::ModelText(text)
        | ServerEvent::Thought(text)
        | ServerEvent::InputTranscript(text)
        | ServerEvent::OutputTranscript(text) => !text.trim().is_empty(),
        ServerEvent::ToolCall { .. }
        | ServerEvent::Interrupted
        | ServerEvent::GenerationComplete
        | ServerEvent::TurnComplete => true,
        ServerEvent::ToolCancellation(ids) => !ids.is_empty(),
        _ => false,
    }
}

pub(super) fn user_audio_active(
    buf: &Arc<Mutex<Vec<i16>>>,
    last_voice: Instant,
    uncommitted_audio: bool,
) -> bool {
    uncommitted_audio
        || last_voice.elapsed() < Duration::from_secs(2)
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

/// Intentional transport refreshes are queued, never interruptions. The owning
/// turn and every queued output sample must finish before replacing the socket.
pub(super) fn intentional_reconnect_ready(
    state: &Reader,
    user_audio_active: bool,
    assistant_audio_playing: bool,
) -> bool {
    !user_audio_active
        && !assistant_audio_playing
        && state.pending.id.is_none()
        && !state.awaiting
        && !state.active
}

pub(super) fn record_activation_deferral(active: bool) {
    if !active {
        return;
    }
    overlay::push_log(
        "(mcp) activation reconnect deferred until user finishes speaking".to_string(),
    );
    telemetry::event(
        "session_reconnect_deferred",
        "speech",
        telemetry::Privacy::Safe,
        serde_json::json!({
            "trigger": "integration_activation",
            "reason": "user_audio_active",
        }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_or_buffered_voice_blocks_a_session_swap() {
        let quiet = Arc::new(Mutex::new(Vec::new()));
        assert!(user_audio_active(&quiet, Instant::now(), false));
        let old = Instant::now() - Duration::from_secs(3);
        assert!(!user_audio_active(&quiet, old, false));
        assert!(user_audio_active(&quiet, old, true));

        let voiced = Arc::new(Mutex::new(vec![300, -300]));
        assert!(user_audio_active(&voiced, old, false));
    }

    #[test]
    fn metadata_does_not_mask_a_silent_generation() {
        assert!(!generation_progress(&ServerEvent::Usage(
            serde_json::json!({})
        )));
        assert!(!generation_progress(&ServerEvent::SessionResumption {
            handle: Some("h".to_string()),
            resumable: true,
        }));
        assert!(generation_progress(&ServerEvent::Thought(
            "working".to_string()
        )));
        assert!(!generation_progress(&ServerEvent::InputTranscript(
            " ".to_string()
        )));
    }

    #[test]
    fn intentional_refresh_waits_for_voice_playback_action_and_turn() {
        let mut state = Reader::default();
        assert!(!intentional_reconnect_ready(&state, true, false));
        assert!(!intentional_reconnect_ready(&state, false, true));

        state.awaiting = true;
        assert!(!intentional_reconnect_ready(&state, false, false));

        state.awaiting = false;
        state.active = true;
        assert!(!intentional_reconnect_ready(&state, false, false));

        state.active = false;
        state.pending.id = Some("in-flight".to_string());
        assert!(!intentional_reconnect_ready(&state, false, false));

        state.pending.id = None;
        assert!(intentional_reconnect_ready(&state, false, false));
    }
}
