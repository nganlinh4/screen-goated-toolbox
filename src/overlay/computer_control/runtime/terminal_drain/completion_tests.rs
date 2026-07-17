use super::*;
use std::sync::mpsc;

#[test]
fn connection_replacement_retires_an_open_final_response_to_idle() {
    let mut state = Reader::default();
    begin_final_response(&mut state, true);
    assert!(retire_for_connection_replacement(&mut state));
    assert!(!state.terminal_drain);
    assert!(!state.terminal_response.is_open());
    assert!(!state.active);
    assert!(!state.awaiting);
    assert!(!retire_for_connection_replacement(&mut state));
}

#[test]
fn connection_replacement_preserves_an_already_delivered_completion_receipt() {
    let mut state = Reader {
        terminal_drain: true,
        terminal_accepted: true,
        terminal_boundary_seen: true,
        terminal_final_response_delivered: true,
        terminal_response: FinalResponseState::Closed,
        turn_summary_emitted: true,
        ..Reader::default()
    };

    assert!(!retire_for_connection_replacement(&mut state));
    assert!(state.terminal_drain);
    assert!(state.terminal_accepted);
    assert!(state.terminal_final_response_delivered);
    assert!(super::super::scripted::has_accepted_completion(&state));
    assert!(!state.active);
    assert!(!state.awaiting);
}

#[test]
fn action_boundary_releases_one_response_and_returns_idle() {
    let mut state = Reader {
        active: true,
        awaiting: true,
        turn_mode: crate::overlay::computer_control::turn_policy::TurnMode::Action,
        ..Reader::default()
    };
    let (exec_tx, exec_rx) = mpsc::channel();
    super::super::reader::handle_event(
        ServerEvent::Audio(vec![1, -1, 2, -2]),
        None,
        &exec_tx,
        &mut state,
    );
    super::super::reader::handle_event(
        ServerEvent::OutputTranscript("unverified completion claim".to_string()),
        None,
        &exec_tx,
        &mut state,
    );
    assert_eq!(state.generation_audio.len(), 4);
    super::super::reader::handle_event(ServerEvent::TurnComplete, None, &exec_tx, &mut state);
    assert!(state.terminal_drain);
    assert!(state.terminal_accepted);
    assert!(state.terminal_final_response_delivered);
    assert_eq!(state.generation_audio.len(), 0);
    assert!(state.reply.is_empty());
    assert!(
        state
            .history
            .iter()
            .any(|entry| entry == "Assistant: unverified completion claim")
    );
    assert!(!state.active);
    assert!(!state.awaiting);
    assert!(state.turn_cleanup_pending.is_some());
    let job = exec_rx.try_recv().expect("turn-retirement job queued");
    assert_eq!(job.name, super::super::RETIRE_TURN);
}

#[test]
fn conversational_boundary_also_retires_turn_owned_resources() {
    let mut state = Reader {
        active: true,
        awaiting: true,
        turn_mode: crate::overlay::computer_control::turn_policy::TurnMode::Conversation,
        ..Reader::default()
    };
    let (exec_tx, exec_rx) = mpsc::channel();
    super::super::reader::handle_event(
        ServerEvent::OutputTranscript("Here is the answer.".to_string()),
        None,
        &exec_tx,
        &mut state,
    );
    super::super::reader::handle_event(ServerEvent::TurnComplete, None, &exec_tx, &mut state);

    assert!(state.terminal_drain);
    assert!(state.terminal_accepted);
    assert!(state.terminal_final_response_delivered);
    assert!(!state.active);
    assert!(!state.awaiting);
    assert!(state.turn_cleanup_pending.is_some());
    assert_eq!(
        exec_rx.try_recv().expect("turn-retirement job queued").name,
        super::super::RETIRE_TURN
    );
}

#[test]
fn action_boundary_still_closes_when_local_cleanup_is_unavailable() {
    let mut state = Reader {
        active: true,
        awaiting: true,
        turn_mode: crate::overlay::computer_control::turn_policy::TurnMode::Action,
        ..Reader::default()
    };
    let (exec_tx, exec_rx) = mpsc::channel();
    drop(exec_rx);
    super::super::reader::handle_event(
        ServerEvent::OutputTranscript("finished response".to_string()),
        None,
        &exec_tx,
        &mut state,
    );
    super::super::reader::handle_event(ServerEvent::TurnComplete, None, &exec_tx, &mut state);
    assert!(state.terminal_drain);
    assert!(state.terminal_accepted);
    assert!(state.terminal_final_response_delivered);
    assert!(state.reply.is_empty());
    assert!(
        state
            .history
            .iter()
            .any(|entry| entry.contains("finished response"))
    );
    assert!(!state.active);
    assert!(!state.awaiting);
    assert_eq!(state.turn_cleanup_pending, None);
}

#[test]
fn silent_action_boundary_closes_without_manufacturing_speech() {
    let (exec_tx, exec_rx) = mpsc::channel();
    let mut state = Reader {
        active: true,
        awaiting: true,
        turn_mode: crate::overlay::computer_control::turn_policy::TurnMode::Action,
        ..Reader::default()
    };
    super::super::reader::handle_event(ServerEvent::TurnComplete, None, &exec_tx, &mut state);
    assert!(state.terminal_drain);
    assert!(state.terminal_accepted);
    assert!(!state.terminal_final_response_delivered);
    assert!(state.reply.is_empty());
    assert!(state.history.is_empty());
    assert!(!super::super::scripted::turn_outcome_acceptable(&state));
    assert_eq!(
        exec_rx.try_recv().expect("turn-retirement job queued").name,
        super::super::RETIRE_TURN
    );
}

#[test]
fn pre_tool_response_closes_without_a_second_generation() {
    let mut state = Reader {
        active: true,
        awaiting: true,
        reply: "The requested change is complete.".to_string(),
        pending_tool_boundary_seen: true,
        ..Reader::default()
    };

    finish_pre_tool_response(&mut state);

    assert!(state.terminal_drain);
    assert!(state.terminal_accepted);
    assert!(state.terminal_final_response_delivered);
    assert!(!state.terminal_response.is_open());
    assert!(!state.active);
    assert!(!state.awaiting);
    assert!(!state.pending_tool_boundary_seen);
    assert!(state.reply.is_empty());
    assert!(
        state
            .history
            .iter()
            .any(|entry| entry.contains("requested change is complete"))
    );
}
