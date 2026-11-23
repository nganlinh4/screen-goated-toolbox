//! Admission of the next typed user turn after the preceding turn has settled.

use super::reader::Reader;

pub(super) fn ready(state: &Reader, audio_playing: bool) -> bool {
    !state.active
        && !state.awaiting
        && !state.terminal_response.is_open()
        && state.pending.id.is_none()
        && state.turn_cleanup_pending.is_none()
        && !audio_playing
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay::computer_control::protocol::ServerEvent;
    use std::sync::mpsc;

    #[test]
    fn next_typed_turn_retires_completed_generation_and_retains_late_event_guard() {
        let mut state = Reader::default();
        let (tx, _rx) = mpsc::channel();
        super::super::reader::handle_event(
            ServerEvent::InputTranscript("first request".to_owned()),
            None,
            &tx,
            &mut state,
        );
        super::super::reader::handle_event(ServerEvent::TurnComplete, None, &tx, &mut state);
        assert!(state.terminal_drain);
        state.turn_cleanup_pending = Some(1);
        assert!(!ready(&state, false)); // turn-owned cleanup must finish first
        state.turn_cleanup_pending = None;
        assert!(ready(&state, false));
        assert!(!ready(&state, true));
        state.input_transcript.begin_epoch();
        super::super::reader::handle_event(
            ServerEvent::InputTranscript("second request".to_owned()),
            None,
            &tx,
            &mut state,
        );
        assert!(!state.terminal_drain);
        assert_eq!(state.last_user_text, "second request");
        assert!(!ready(&state, false));
    }

    #[test]
    fn unfinished_final_response_and_pending_action_block_next_turn() {
        let mut state = Reader::default();
        super::super::terminal_drain::begin_final_response(&mut state, true);
        assert!(!ready(&state, false));
        state.active = false;
        state.awaiting = false;
        assert!(!ready(&state, false));
        state.terminal_response = super::super::terminal_drain::FinalResponseState::Closed;
        state.pending.id = Some("active-job".to_owned());
        assert!(!ready(&state, false));
    }
}
