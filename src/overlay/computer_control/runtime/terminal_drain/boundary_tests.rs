use super::*;
use serde_json::json;
use std::sync::mpsc;

#[test]
fn accepted_terminal_tool_allows_one_final_response_then_rejects_everything() {
    let mut state = Reader::default();
    begin_final_response(&mut state, true);
    assert_eq!(
        state.terminal_response,
        FinalResponseState::AwaitingPriorBoundary
    );
    let (exec_tx, exec_rx) = mpsc::channel();
    super::super::reader::handle_event(
        ServerEvent::OutputTranscript("final response".to_string()),
        None,
        &exec_tx,
        &mut state,
    );
    assert_eq!(state.reply, "final response");
    super::super::reader::handle_event(ServerEvent::TurnComplete, None, &exec_tx, &mut state);
    assert!(!state.terminal_response.is_open());
    assert!(!state.active);
    assert!(!state.awaiting);
    assert!(
        state
            .history
            .iter()
            .any(|entry| entry == "Assistant: final response")
    );

    super::super::reader::handle_event(
        ServerEvent::OutputTranscript("duplicate".to_string()),
        None,
        &exec_tx,
        &mut state,
    );
    super::super::reader::handle_event(
        ServerEvent::ToolCall {
            id: "late-call".to_string(),
            name: "future_tool".to_string(),
            args: json!({}),
        },
        None,
        &exec_tx,
        &mut state,
    );
    assert!(state.reply.is_empty());
    assert!(exec_rx.try_recv().is_err());
    assert_eq!(state.terminal_dropped_events, 2);
    let response = state.immediate_tool_responses.pop_front().unwrap();
    assert_eq!(response.0, "late-call");
    assert_eq!(response.2["error"]["code"], "turn_already_completed");
}

#[test]
fn post_tool_output_is_owned_without_an_intermediate_boundary() {
    let mut state = Reader::default();
    begin_final_response(&mut state, true);
    assert_eq!(
        state.terminal_response,
        FinalResponseState::AwaitingPriorBoundary
    );
    assert!(state.active);
    assert!(state.awaiting);
    assert!(!state.terminal_boundary_seen);

    let (exec_tx, _exec_rx) = mpsc::channel();
    super::super::reader::handle_event(
        ServerEvent::OutputTranscript("owned response".to_string()),
        None,
        &exec_tx,
        &mut state,
    );
    assert_eq!(state.reply, "owned response");
    assert_eq!(state.terminal_response, FinalResponseState::Streaming);
    super::super::reader::handle_event(ServerEvent::TurnComplete, None, &exec_tx, &mut state);
    assert_eq!(state.terminal_response, FinalResponseState::Closed);
    assert!(state.terminal_boundary_seen);
}

#[test]
fn final_boundary_without_output_closes_after_prior_boundary_was_seen() {
    let mut state = Reader {
        active: true,
        pending: super::super::reader::Pending {
            id: Some("terminal-call".to_string()),
            ..super::super::reader::Pending::default()
        },
        ..Reader::default()
    };
    let (exec_tx, _exec_rx) = mpsc::channel();
    super::super::reader::handle_event(ServerEvent::TurnComplete, None, &exec_tx, &mut state);
    assert!(state.pending_tool_boundary_seen);
    begin_final_response(&mut state, true);
    assert_eq!(state.terminal_response, FinalResponseState::AwaitingOutput);
    assert!(handle(&ServerEvent::TurnComplete, None, &mut state));
    assert_eq!(state.terminal_response, FinalResponseState::Closed);
    assert!(!state.active);
    assert!(!state.awaiting);
    assert!(state.terminal_boundary_seen);
}

#[test]
fn queued_prior_boundary_cannot_close_or_suppress_final_output() {
    let mut state = Reader::default();
    begin_final_response(&mut state, true);
    assert_eq!(
        state.terminal_response,
        FinalResponseState::AwaitingPriorBoundary
    );
    assert!(handle(&ServerEvent::TurnComplete, None, &mut state));
    assert_eq!(state.terminal_response, FinalResponseState::AwaitingOutput);
    assert!(!state.terminal_boundary_seen);

    let (exec_tx, _exec_rx) = mpsc::channel();
    super::super::reader::handle_event(
        ServerEvent::OutputTranscript("final after prior boundary".to_string()),
        None,
        &exec_tx,
        &mut state,
    );
    assert_eq!(state.terminal_response, FinalResponseState::Streaming);
    super::super::reader::handle_event(ServerEvent::TurnComplete, None, &exec_tx, &mut state);
    assert_eq!(state.terminal_response, FinalResponseState::Closed);
    assert!(state.terminal_boundary_seen);
}

#[test]
fn interruption_then_new_transcript_guards_the_late_old_boundary() {
    let mut state = Reader::default();
    begin_final_response(&mut state, true);
    let (exec_tx, _exec_rx) = mpsc::channel();
    super::super::reader::handle_event(ServerEvent::Interrupted, None, &exec_tx, &mut state);
    assert!(!state.terminal_boundary_seen);
    super::super::reader::handle_event(
        ServerEvent::InputTranscript("new turn".to_string()),
        None,
        &exec_tx,
        &mut state,
    );
    assert!(state.ignore_stale_boundary);
    assert!(state.active);
    super::super::reader::handle_event(ServerEvent::TurnComplete, None, &exec_tx, &mut state);
    assert!(!state.ignore_stale_boundary);
    assert!(state.active);
    assert!(state.awaiting);
}

#[test]
fn silent_terminal_response_expires_to_idle() {
    let mut state = Reader::default();
    begin_final_response(&mut state, true);
    state.terminal_activity_at = Some(std::time::Instant::now() - FINAL_RESPONSE_IDLE_TIMEOUT);
    expire_after_socket_drained(&mut state, None);
    assert_eq!(state.terminal_response, FinalResponseState::Closed);
    assert!(!state.active);
    assert!(!state.awaiting);
    assert!(!state.terminal_boundary_seen);
}

#[test]
fn partial_output_without_a_boundary_expires_to_idle() {
    let mut state = Reader::default();
    begin_final_response(&mut state, true);
    let (exec_tx, _exec_rx) = mpsc::channel();
    super::super::reader::handle_event(
        ServerEvent::OutputTranscript("partial final response".to_string()),
        None,
        &exec_tx,
        &mut state,
    );
    assert_eq!(state.terminal_response, FinalResponseState::Streaming);
    state.terminal_activity_at = Some(std::time::Instant::now() - FINAL_RESPONSE_IDLE_TIMEOUT);

    expire_after_socket_drained(&mut state, None);

    assert_eq!(state.terminal_response, FinalResponseState::Closed);
    assert!(!state.active);
    assert!(!state.awaiting);
    assert!(
        state
            .history
            .iter()
            .any(|entry| entry == "Assistant: partial final response")
    );
}

#[test]
fn queued_output_at_deadline_refreshes_activity_before_empty_read_expiry() {
    let mut state = Reader::default();
    begin_final_response(&mut state, true);
    state.terminal_activity_at = Some(std::time::Instant::now() - FINAL_RESPONSE_IDLE_TIMEOUT);
    let (exec_tx, _exec_rx) = mpsc::channel();

    // The socket loop handles every readable frame before it invokes the
    // empty-read expiry hook. This queued fragment therefore owns the turn.
    super::super::reader::handle_event(
        ServerEvent::OutputTranscript("queued final response".to_string()),
        None,
        &exec_tx,
        &mut state,
    );
    expire_after_socket_drained(&mut state, None);

    assert_eq!(state.terminal_response, FinalResponseState::Streaming);
    assert!(state.active);
    assert!(state.awaiting);
    assert_eq!(state.reply, "queued final response");
}
