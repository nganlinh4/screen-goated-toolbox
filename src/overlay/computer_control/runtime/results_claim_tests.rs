use super::{Reader, close_terminal_blocker};

#[test]
fn failed_done_claim_never_enters_history_or_the_blocker_utterance() {
    let mut state = Reader {
        active: true,
        awaiting: true,
        reply: "unsupported completion claim".into(),
        ..Reader::default()
    };
    super::speech_events::audio(&mut state, &[1, -1, 2, -2], None);
    assert!(state.assistant_utterance_id.is_some());

    close_terminal_blocker(&mut state, None, true);

    assert_eq!(state.generation_audio.len(), 0);
    assert!(state.reply.is_empty());
    assert!(state.history.is_empty());
    assert!(state.assistant_utterance_id.is_none());
    assert!(state.terminal_response.is_open());
    assert!(!state.terminal_accepted);
}
