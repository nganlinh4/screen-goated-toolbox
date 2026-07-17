use super::*;
use crate::overlay::computer_control::protocol::parse_server_message;
use serde_json::json;
use std::sync::mpsc;

#[test]
fn new_transcript_retires_latch_and_unseen_boundary_is_guarded() {
    let mut state = Reader {
        terminal_drain: true,
        ..Reader::default()
    };
    assert!(!handle(
        &ServerEvent::InputTranscript("new request".to_string()),
        None,
        &mut state
    ));
    assert!(!state.terminal_drain);
    assert!(state.ignore_stale_boundary);
}

#[test]
fn empty_transcript_cannot_retire_a_closed_generation() {
    let mut state = Reader {
        terminal_drain: true,
        terminal_accepted: true,
        ..Reader::default()
    };
    assert!(handle(
        &ServerEvent::InputTranscript("  ".to_string()),
        None,
        &mut state
    ));
    assert!(state.terminal_drain);
    assert!(state.terminal_accepted);
    assert!(!state.ignore_stale_boundary);
}

#[test]
fn ordinary_answer_boundary_latches_against_late_output_and_tools() {
    let mut state = Reader {
        active: true,
        awaiting: true,
        recovery_owed: true,
        ..Reader::default()
    };
    let (exec_tx, exec_rx) = mpsc::channel();
    super::super::reader::handle_event(ServerEvent::TurnComplete, None, &exec_tx, &mut state);
    assert!(state.terminal_drain);
    assert!(state.terminal_accepted);
    super::super::reader::handle_event(
        ServerEvent::OutputTranscript("late".to_string()),
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
    let cleanup = exec_rx.try_recv().expect("turn-retirement job queued");
    assert_eq!(cleanup.name, super::super::RETIRE_TURN);
    assert!(exec_rx.try_recv().is_err());
    assert_eq!(state.immediate_tool_responses.len(), 1);
}

#[test]
fn model_rationale_cannot_replace_the_committed_user_goal() {
    let mut state = Reader::default();
    let (exec_tx, exec_rx) = mpsc::channel();
    super::super::reader::handle_event(
        ServerEvent::InputTranscript("committed user goal".to_string()),
        None,
        &exec_tx,
        &mut state,
    );
    super::super::reader::handle_event(
        ServerEvent::Thought("different model rationale".to_string()),
        None,
        &exec_tx,
        &mut state,
    );
    super::super::reader::handle_event(
        ServerEvent::ToolCall {
            id: "call".to_string(),
            name: "future_tool".to_string(),
            args: json!({}),
        },
        None,
        &exec_tx,
        &mut state,
    );
    let job = exec_rx.try_recv().expect("tool job");
    assert_eq!(job.task, "committed user goal");
    assert_eq!(job.user_text, "committed user goal");
    assert_eq!(state.last_cmd, "committed user goal");
}

#[test]
fn unaccepted_action_goal_remains_in_the_next_user_task() {
    let mut state = Reader::default();
    let (exec_tx, exec_rx) = mpsc::channel();
    super::super::reader::handle_event(
        ServerEvent::InputTranscript("prepare the full report".to_string()),
        None,
        &exec_tx,
        &mut state,
    );
    state.turn_mode = crate::overlay::computer_control::turn_policy::TurnMode::Action;
    super::super::reader_policy::begin_terminal_drain(&mut state, false, true);
    state.input_transcript.begin_epoch();
    super::super::reader::handle_event(
        ServerEvent::InputTranscript("also correct the priority".to_string()),
        None,
        &exec_tx,
        &mut state,
    );
    super::super::reader::handle_event(
        ServerEvent::ToolCall {
            id: "follow-up".to_string(),
            name: "future_tool".to_string(),
            args: json!({}),
        },
        None,
        &exec_tx,
        &mut state,
    );

    let job = exec_rx.try_recv().expect("follow-up tool job");
    assert!(job.task.contains("prepare the full report"));
    assert!(job.task.contains("also correct the priority"));
    assert_eq!(job.user_text, "also correct the priority");
}

#[test]
fn accepted_action_goal_does_not_bind_an_unrelated_new_turn() {
    let mut state = Reader::default();
    let (exec_tx, exec_rx) = mpsc::channel();
    super::super::reader::handle_event(
        ServerEvent::InputTranscript("finish the first task".to_string()),
        None,
        &exec_tx,
        &mut state,
    );
    state.turn_mode = crate::overlay::computer_control::turn_policy::TurnMode::Action;
    super::super::reader_policy::begin_terminal_drain(&mut state, true, true);
    state.input_transcript.begin_epoch();
    super::super::reader::handle_event(
        ServerEvent::InputTranscript("start a separate task".to_string()),
        None,
        &exec_tx,
        &mut state,
    );
    super::super::reader::handle_event(
        ServerEvent::ToolCall {
            id: "new-task".to_string(),
            name: "future_tool".to_string(),
            args: json!({}),
        },
        None,
        &exec_tx,
        &mut state,
    );

    let job = exec_rx.try_recv().expect("new task tool job");
    assert_eq!(job.task, "start a separate task");
}

#[test]
fn barge_in_during_an_active_action_retains_both_user_turns() {
    let mut state = Reader::default();
    let (exec_tx, exec_rx) = mpsc::channel();
    super::super::reader::handle_event(
        ServerEvent::InputTranscript("organize the records".to_string()),
        None,
        &exec_tx,
        &mut state,
    );
    state.turn_mode = crate::overlay::computer_control::turn_policy::TurnMode::Action;
    state.input_transcript.begin_epoch();
    super::super::reader::handle_event(
        ServerEvent::InputTranscript("keep the original order too".to_string()),
        None,
        &exec_tx,
        &mut state,
    );
    super::super::reader::handle_event(
        ServerEvent::ToolCall {
            id: "steered".to_string(),
            name: "future_tool".to_string(),
            args: json!({}),
        },
        None,
        &exec_tx,
        &mut state,
    );

    let job = exec_rx.try_recv().expect("steered tool job");
    assert!(job.task.contains("organize the records"));
    assert!(job.task.contains("keep the original order too"));
}

#[test]
fn first_real_output_clears_only_recovery_debt() {
    let mut state = Reader {
        active: true,
        awaiting: true,
        recovery_owed: true,
        ..Reader::default()
    };
    let (exec_tx, _exec_rx) = mpsc::channel();
    super::super::reader::handle_event(
        ServerEvent::OutputTranscript("answer".to_string()),
        None,
        &exec_tx,
        &mut state,
    );
    assert!(state.active);
    assert!(state.awaiting);
    assert!(!state.recovery_owed);
    assert!(!super::super::reader_policy::recovery_due(&state));
}

#[test]
fn done_with_coalesced_boundary_cannot_enable_late_tools() {
    let mut state = Reader::default();
    let (exec_tx, _exec_rx) = mpsc::channel();
    let raw = r#"{"serverContent":{"turnComplete":true},"toolCall":{"functionCalls":[{"id":"done-call","name":"done","args":{"summary":"complete"}}]}}"#;
    let events = parse_server_message(raw);
    super::super::reader_policy::begin_terminal_drain(&mut state, true, false);
    for event in events
        .into_iter()
        .filter(|event| matches!(event, ServerEvent::TurnComplete))
    {
        super::super::reader::handle_event(event, None, &exec_tx, &mut state);
    }
    assert!(state.terminal_drain);
    super::super::reader::handle_event(
        ServerEvent::ToolCall {
            id: "late-call".to_string(),
            name: "future_capability".to_string(),
            args: json!({}),
        },
        None,
        &exec_tx,
        &mut state,
    );
    assert!(state.terminal_drain);
    assert_eq!(state.immediate_tool_responses.len(), 1);
}
