use super::*;
use std::sync::mpsc;

#[test]
fn new_transcript_survives_the_old_generation_boundary() {
    let mut state = Reader {
        terminal_drain: true,
        ..Reader::default()
    };
    let (exec_tx, _exec_rx) = mpsc::channel();
    super::super::reader::handle_event(
        ServerEvent::InputTranscript("new request".to_string()),
        None,
        &exec_tx,
        &mut state,
    );
    assert!(state.active);
    assert!(state.awaiting);
    super::super::reader::handle_event(ServerEvent::TurnComplete, None, &exec_tx, &mut state);
    assert!(state.active);
    assert!(state.awaiting);
    super::super::reader::handle_event(
        ServerEvent::OutputTranscript("fresh answer".to_string()),
        None,
        &exec_tx,
        &mut state,
    );
    super::super::reader::handle_event(ServerEvent::TurnComplete, None, &exec_tx, &mut state);
    assert!(!state.active);
    assert!(!state.awaiting);
}

#[test]
fn transcript_fragments_update_one_turn_without_cancelling_its_action() {
    let mut state = Reader::default();
    let (exec_tx, _exec_rx) = mpsc::channel();
    super::super::reader::handle_event(
        ServerEvent::InputTranscript("perform the fir".to_string()),
        None,
        &exec_tx,
        &mut state,
    );
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    state.pending = super::super::reader::Pending {
        id: Some("current-action".to_string()),
        tool: Some("future_operation".to_string()),
        turn_id: Some(1),
        cancelled: false,
        cancel: Some(cancel.clone()),
    };

    super::super::reader::handle_event(
        ServerEvent::InputTranscript("perform the first operation".to_string()),
        None,
        &exec_tx,
        &mut state,
    );

    assert_eq!(state.last_cmd, "perform the first operation");
    assert_eq!(state.last_user_text, state.last_cmd);
    assert_eq!(
        state
            .history
            .iter()
            .filter(|entry| entry.starts_with("User:"))
            .count(),
        1
    );
    assert!(!state.pending.cancelled);
    assert!(!cancel.load(std::sync::atomic::Ordering::SeqCst));
}

#[test]
fn fresh_input_epoch_starts_a_new_turn_after_completion() {
    let mut state = Reader::default();
    let (exec_tx, _exec_rx) = mpsc::channel();
    super::super::reader::handle_event(
        ServerEvent::InputTranscript("first request".to_string()),
        None,
        &exec_tx,
        &mut state,
    );
    super::super::reader::handle_event(ServerEvent::TurnComplete, None, &exec_tx, &mut state);
    state.input_transcript.begin_epoch();
    super::super::reader::handle_event(
        ServerEvent::InputTranscript("second request".to_string()),
        None,
        &exec_tx,
        &mut state,
    );

    assert_eq!(state.last_cmd, "second request");
    assert_eq!(
        state
            .history
            .iter()
            .filter(|entry| entry.starts_with("User:"))
            .count(),
        2
    );
}

#[test]
fn late_cumulative_asr_revision_updates_history_without_reviving_the_turn() {
    let mut state = Reader::default();
    let (exec_tx, _exec_rx) = mpsc::channel();
    super::super::reader::handle_event(
        ServerEvent::InputTranscript("perform the fir".to_string()),
        None,
        &exec_tx,
        &mut state,
    );
    super::super::reader::handle_event(ServerEvent::TurnComplete, None, &exec_tx, &mut state);
    assert!(state.terminal_drain);

    super::super::reader::handle_event(
        ServerEvent::InputTranscript("perform the first operation".to_string()),
        None,
        &exec_tx,
        &mut state,
    );

    assert!(state.terminal_drain);
    assert!(!state.active);
    assert!(!state.awaiting);
    assert_eq!(state.last_cmd, "perform the first operation");
    assert_eq!(
        state
            .history
            .iter()
            .filter(|entry| entry.starts_with("User:"))
            .count(),
        1
    );
}
