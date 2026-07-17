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
    assert!(state.terminal_final_response_delivered);
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
    assert_eq!(state.terminal_effectful_dropped_events, 2);
    let response = state.immediate_tool_responses.pop_front().unwrap();
    assert_eq!(response.0, "late-call");
    assert_eq!(response.2["error"]["code"], "turn_already_completed");
}

#[test]
fn terminal_drop_counters_separate_protocol_chatter_from_late_effects() {
    let mut state = Reader {
        terminal_drain: true,
        terminal_accepted: true,
        ..Reader::default()
    };
    assert!(handle(
        &ServerEvent::Thought("late rationale".to_string()),
        None,
        &mut state
    ));
    assert!(handle(
        &ServerEvent::ToolCancellation(vec!["closed call".to_string()]),
        None,
        &mut state
    ));
    assert_eq!(state.terminal_dropped_events, 2);
    assert_eq!(state.terminal_effectful_dropped_events, 0);

    assert!(handle(&ServerEvent::Audio(vec![1, 2]), None, &mut state));
    assert_eq!(state.terminal_dropped_events, 3);
    assert_eq!(state.terminal_effectful_dropped_events, 1);
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
fn generation_completion_waits_for_the_distinct_turn_boundary() {
    let mut state = Reader::default();
    begin_final_response(&mut state, true);
    let (exec_tx, _exec_rx) = mpsc::channel();
    super::super::reader::handle_event(
        ServerEvent::OutputTranscript("bounded final".to_string()),
        None,
        &exec_tx,
        &mut state,
    );

    super::super::reader::handle_event(ServerEvent::GenerationComplete, None, &exec_tx, &mut state);
    assert!(state.terminal_response.is_open());
    assert!(state.terminal_generation_complete);
    assert!(state.terminal_final_response_delivered);
    assert!(state.active);
    assert!(state.awaiting);
    assert!(!state.terminal_boundary_seen);

    // Output transcription is an independent stream and may trail the model's
    // generation-complete signal. It still belongs to the same final response.
    super::super::reader::handle_event(
        ServerEvent::OutputTranscript(" metadata".to_string()),
        None,
        &exec_tx,
        &mut state,
    );
    assert_eq!(state.reply, "bounded final metadata");

    super::super::reader::handle_event(ServerEvent::TurnComplete, None, &exec_tx, &mut state);
    assert!(!state.terminal_response.is_open());
    assert!(state.terminal_boundary_seen);
    assert!(state.terminal_final_response_delivered);
}

#[test]
fn missing_turn_boundary_fallback_accepts_a_completed_text_only_generation() {
    let mut state = Reader::default();
    begin_final_response(&mut state, true);
    let (exec_tx, _exec_rx) = mpsc::channel();
    super::super::reader::handle_event(
        ServerEvent::OutputTranscript("text-only final".to_string()),
        None,
        &exec_tx,
        &mut state,
    );
    super::super::reader::handle_event(ServerEvent::GenerationComplete, None, &exec_tx, &mut state);
    state.terminal_activity_at = Some(std::time::Instant::now() - FINAL_RESPONSE_IDLE_TIMEOUT);

    expire_after_socket_drained(&mut state, None);

    assert_eq!(state.terminal_response, FinalResponseState::Closed);
    assert!(state.terminal_generation_complete);
    assert!(state.terminal_final_response_delivered);
    assert!(!state.terminal_boundary_seen);
}

#[test]
fn reconnect_after_completed_final_generation_preserves_exactly_one_delivery() {
    let mut state = Reader::default();
    begin_final_response(&mut state, true);
    let (exec_tx, _exec_rx) = mpsc::channel();
    super::super::reader::handle_event(
        ServerEvent::OutputTranscript("final before transport loss".to_string()),
        None,
        &exec_tx,
        &mut state,
    );
    super::super::reader::handle_event(ServerEvent::GenerationComplete, None, &exec_tx, &mut state);
    assert!(state.terminal_response.is_open());

    assert!(!retire_for_connection_replacement(&mut state));
    assert_eq!(state.terminal_response, FinalResponseState::Closed);
    assert!(state.terminal_drain);
    assert!(state.terminal_accepted);
    assert!(state.terminal_final_response_delivered);
}

#[test]
fn user_turn_between_generation_and_turn_boundaries_guards_the_late_boundary() {
    let mut state = Reader::default();
    begin_final_response(&mut state, true);
    let (exec_tx, _exec_rx) = mpsc::channel();
    super::super::reader::handle_event(
        ServerEvent::OutputTranscript("final".to_string()),
        None,
        &exec_tx,
        &mut state,
    );
    super::super::reader::handle_event(ServerEvent::GenerationComplete, None, &exec_tx, &mut state);

    state.input_transcript.begin_epoch();
    super::super::reader::handle_event(
        ServerEvent::InputTranscript("next request".to_string()),
        None,
        &exec_tx,
        &mut state,
    );
    assert!(!state.terminal_drain);
    assert!(state.ignore_stale_boundary);
    assert!(state.active);

    super::super::reader::handle_event(ServerEvent::TurnComplete, None, &exec_tx, &mut state);
    assert!(!state.ignore_stale_boundary);
    assert!(state.active);
    assert!(state.awaiting);
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
fn queued_prior_generation_and_turn_boundaries_keep_the_final_window_open() {
    let mut state = Reader::default();
    begin_final_response(&mut state, true);
    let (exec_tx, _exec_rx) = mpsc::channel();

    super::super::reader::handle_event(ServerEvent::GenerationComplete, None, &exec_tx, &mut state);
    super::super::reader::handle_event(ServerEvent::TurnComplete, None, &exec_tx, &mut state);

    assert!(state.terminal_response.is_open());
    assert!(state.active);
    assert!(state.awaiting);
    assert!(!state.terminal_boundary_seen);

    super::super::reader::handle_event(
        ServerEvent::OutputTranscript("owned final output".to_string()),
        None,
        &exec_tx,
        &mut state,
    );
    assert_eq!(state.terminal_response, FinalResponseState::Streaming);
    super::super::reader::handle_event(ServerEvent::TurnComplete, None, &exec_tx, &mut state);

    assert_eq!(state.terminal_response, FinalResponseState::Closed);
    assert!(!state.terminal_generation_complete);
    assert!(state.terminal_final_response_delivered);
    assert!(state.terminal_boundary_seen);
}

#[test]
fn output_between_queued_prior_boundaries_remains_owned() {
    let mut state = Reader::default();
    begin_final_response(&mut state, true);
    let (exec_tx, _exec_rx) = mpsc::channel();

    super::super::reader::handle_event(ServerEvent::GenerationComplete, None, &exec_tx, &mut state);
    super::super::reader::handle_event(
        ServerEvent::OutputTranscript("streaming final".to_string()),
        None,
        &exec_tx,
        &mut state,
    );
    super::super::reader::handle_event(ServerEvent::TurnComplete, None, &exec_tx, &mut state);

    assert_eq!(state.terminal_response, FinalResponseState::Streaming);
    assert_eq!(state.reply, "streaming final");
    assert!(!state.terminal_boundary_seen);

    super::super::reader::handle_event(ServerEvent::GenerationComplete, None, &exec_tx, &mut state);
    super::super::reader::handle_event(ServerEvent::TurnComplete, None, &exec_tx, &mut state);

    assert_eq!(state.terminal_response, FinalResponseState::Closed);
    assert!(state.terminal_final_response_delivered);
}

#[test]
fn owned_generation_supersedes_an_omitted_prior_turn_boundary() {
    let mut state = Reader::default();
    begin_final_response(&mut state, true);
    let (exec_tx, _exec_rx) = mpsc::channel();

    super::super::reader::handle_event(ServerEvent::GenerationComplete, None, &exec_tx, &mut state);
    super::super::reader::handle_event(
        ServerEvent::OutputTranscript("owned output".to_string()),
        None,
        &exec_tx,
        &mut state,
    );
    super::super::reader::handle_event(ServerEvent::GenerationComplete, None, &exec_tx, &mut state);
    super::super::reader::handle_event(ServerEvent::TurnComplete, None, &exec_tx, &mut state);

    assert_eq!(state.terminal_response, FinalResponseState::Closed);
    assert!(state.terminal_generation_complete);
    assert!(state.terminal_final_response_delivered);
    assert!(state.terminal_boundary_seen);
}

#[test]
fn ordinary_post_tool_generation_still_closes_at_its_turn_boundary() {
    let mut state = Reader {
        pending_tool_boundary_seen: true,
        ..Reader::default()
    };
    begin_final_response(&mut state, true);
    let (exec_tx, _exec_rx) = mpsc::channel();

    super::super::reader::handle_event(
        ServerEvent::OutputTranscript("ordinary final".to_string()),
        None,
        &exec_tx,
        &mut state,
    );
    super::super::reader::handle_event(ServerEvent::GenerationComplete, None, &exec_tx, &mut state);
    super::super::reader::handle_event(ServerEvent::TurnComplete, None, &exec_tx, &mut state);

    assert_eq!(state.terminal_response, FinalResponseState::Closed);
    assert!(state.terminal_generation_complete);
    assert!(state.terminal_final_response_delivered);
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
    let mut state = Reader {
        turn_tools: vec!["act".to_string()],
        turn_summary_emitted: true,
        ..Reader::default()
    };
    begin_final_response(&mut state, true);
    state.terminal_activity_at = Some(std::time::Instant::now() - FINAL_RESPONSE_IDLE_TIMEOUT);
    expire_after_socket_drained(&mut state, None);
    assert_eq!(state.terminal_response, FinalResponseState::Closed);
    assert!(!state.active);
    assert!(!state.awaiting);
    assert!(!state.terminal_boundary_seen);
    assert!(!state.terminal_final_response_delivered);
    assert!(!super::super::scripted::turn_outcome_acceptable(&state));
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
    assert!(!state.terminal_final_response_delivered);
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
