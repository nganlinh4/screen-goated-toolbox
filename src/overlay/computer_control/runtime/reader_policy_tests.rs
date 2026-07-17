use super::*;
use crate::overlay::computer_control::runtime::reader::Pending;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[test]
fn any_new_user_turn_cancels_pending_work_and_starts_fresh() {
    let cancel = Arc::new(AtomicBool::new(false));
    let mut state = Reader {
        pending: Pending {
            id: Some("in-flight".to_string()),
            tool: Some("future_operation".to_string()),
            turn_id: Some(1),
            cancelled: false,
            cancel: Some(cancel.clone()),
        },
        active: true,
        awaiting: true,
        turn_mode: turn_policy::TurnMode::Action,
        ..Reader::default()
    };

    assert!(apply_user_turn_policy(&mut state, "new turn"));
    assert!(cancel.load(Ordering::SeqCst));
    assert!(state.pending.cancelled);
    assert!(state.active);
    assert!(state.recovery_owed);
    assert!(state.ignore_stale_boundary);
    assert!(state.reconciliation_required);
    assert_eq!(state.turn_mode, turn_policy::TurnMode::Conversation);
}

#[test]
fn concurrent_tool_call_is_not_run_and_waits_behind_its_owner() {
    let mut state = Reader {
        pending: Pending {
            id: Some("owner".to_string()),
            tool: Some("research_web".to_string()),
            ..Pending::default()
        },
        active: true,
        awaiting: true,
        ..Reader::default()
    };
    let action = telemetry::ActionTrace {
        action_id: 2,
        turn_id: 1,
    };

    assert!(guard_tool_call(
        &mut state,
        "later",
        "list_files",
        &serde_json::json!({"path": "C:\\absolute"}),
        action,
    ));

    assert_eq!(state.pending.id.as_deref(), Some("owner"));
    assert_eq!(state.immediate_tool_responses.len(), 1);
    let response = &state.immediate_tool_responses[0].2;
    assert_eq!(response["status"], "blocked_tool_call_in_flight");
    assert_eq!(response["executed"], false);
    assert_eq!(response["effect_may_have_occurred"], false);
    assert_eq!(response["error"]["code"], "tool_call_in_flight");
}

#[test]
fn prior_turn_tool_call_waits_for_reconciliation_instead_of_running_or_claiming_done() {
    let mut state = Reader {
        pending: Pending {
            id: Some("prior-owner".to_string()),
            tool: Some("focus_window".to_string()),
            turn_id: Some(4),
            cancelled: true,
            ..Pending::default()
        },
        reconciliation_required: true,
        ..Reader::default()
    };
    let action = telemetry::ActionTrace {
        action_id: 8,
        turn_id: 5,
    };

    assert!(guard_tool_call(
        &mut state,
        "new-done",
        "done",
        &serde_json::json!({"summary": "stale"}),
        action,
    ));
    let response = &state.immediate_tool_responses[0].2;
    assert_eq!(response["status"], "blocked_prior_action_settling");
    assert_eq!(response["error"]["code"], "prior_action_settling");
    assert_eq!(response["effect_may_have_occurred"], false);
}

#[test]
fn new_user_turn_discards_buffered_responses_from_superseded_generation() {
    let mut state = Reader {
        immediate_tool_responses: std::collections::VecDeque::from([(
            "old".to_string(),
            "future_tool".to_string(),
            serde_json::json!({"ok": false}),
        )]),
        ..Reader::default()
    };

    apply_user_turn_policy(&mut state, "new turn");

    assert!(state.immediate_tool_responses.is_empty());
}

#[test]
fn a_model_boundary_finishes_answers_and_actions_without_self_reviving() {
    let mut answer = Reader {
        active: true,
        turn_mode: turn_policy::TurnMode::Conversation,
        think_start: Some(Instant::now()),
        ..Reader::default()
    };
    assert_eq!(
        finish_at_model_boundary(&mut answer),
        BoundaryOutcome::ConversationComplete
    );
    assert!(!answer.active);
    assert!(answer.think_start.is_none());

    let mut action = Reader {
        active: true,
        turn_mode: turn_policy::TurnMode::Action,
        think_start: Some(Instant::now()),
        ..Reader::default()
    };
    assert_eq!(
        finish_at_model_boundary(&mut action),
        BoundaryOutcome::ActionComplete
    );
    assert!(!action.active);
    assert!(action.think_start.is_none());

    let mut in_flight = Reader {
        pending: Pending {
            id: Some("tool-call".to_string()),
            ..Pending::default()
        },
        active: true,
        turn_mode: turn_policy::TurnMode::Action,
        ..Reader::default()
    };
    assert_eq!(
        finish_at_model_boundary(&mut in_flight),
        BoundaryOutcome::PendingTool
    );
    assert!(in_flight.active);
}

#[test]
fn transport_uncertainty_allows_observation_but_blocks_effects_and_done() {
    let action = telemetry::ActionTrace {
        action_id: 1,
        turn_id: 1,
    };
    let mut state = Reader {
        reconciliation_required: true,
        ..Reader::default()
    };

    assert!(!guard_tool_call(
        &mut state,
        "read-id",
        "observe",
        &serde_json::json!({}),
        action,
    ));
    assert!(guard_tool_call(
        &mut state,
        "write-id",
        "edit_text_file",
        &serde_json::json!({}),
        action,
    ));
    assert!(guard_tool_call(
        &mut state,
        "done-id",
        "done",
        &serde_json::json!({}),
        action,
    ));
    assert_eq!(state.immediate_tool_responses.len(), 2);
    assert!(state.reconciliation_required);
}

#[test]
fn new_user_turn_retires_terminal_latch_and_guards_one_stale_boundary() {
    let mut state = Reader {
        active: true,
        awaiting: true,
        reasoning: "old generation".to_string(),
        thinking: "old thought".to_string(),
        ..Reader::default()
    };
    begin_terminal_drain(&mut state, true, false);
    assert!(state.terminal_drain);
    assert!(state.terminal_accepted);
    assert!(!state.active);
    assert!(state.reasoning.is_empty());
    assert!(state.thinking.is_empty());

    apply_user_turn_policy(&mut state, "new user turn");
    assert!(state.terminal_drain);
    assert!(state.active);
    assert!(state.awaiting);

    assert!(retire_terminal_for_user_turn(&mut state));
    assert!(!state.terminal_drain);
    assert!(!state.terminal_accepted);
    assert!(state.active);
    assert!(state.awaiting);
    assert!(consume_stale_boundary(&mut state));
    assert!(!consume_stale_boundary(&mut state));
}

#[test]
fn new_generation_progress_disarms_the_stale_boundary_guard() {
    let mut state = Reader {
        ignore_stale_boundary: true,
        awaiting: true,
        recovery_owed: true,
        ..Reader::default()
    };
    record_generation_progress(&mut state);
    assert!(!consume_stale_boundary(&mut state));
    assert!(!state.recovery_owed);
    assert!(!recovery_due(&state));
}

#[test]
fn only_substantive_server_output_clears_recovery_debt() {
    assert!(!is_real_generation_progress(
        &ServerEvent::Audio(Vec::new())
    ));
    assert!(!is_real_generation_progress(
        &ServerEvent::OutputTranscript("  ".to_string())
    ));
    assert!(is_real_generation_progress(&ServerEvent::Thought(
        "working".to_string()
    )));
    assert!(is_real_generation_progress(&ServerEvent::ToolCall {
        id: "call".to_string(),
        name: "future_tool".to_string(),
        args: serde_json::json!({}),
    }));

    let state = Reader {
        awaiting: true,
        recovery_owed: true,
        ..Reader::default()
    };
    assert!(recovery_due(&state));
}

#[test]
fn selected_capability_sets_lifecycle_without_parsing_user_language() {
    let request = "unclassified input";
    let mut state = Reader {
        last_user_text: request.to_string(),
        ..Reader::default()
    };
    apply_user_turn_policy(&mut state, request);
    assert_eq!(state.turn_mode, turn_policy::TurnMode::Conversation);
    refine_turn_mode(
        &mut state,
        "Perform the requested interface action",
        "click_target",
    );
    assert_eq!(state.turn_mode, turn_policy::TurnMode::Action);

    let mut observation = Reader::default();
    refine_turn_mode(&mut observation, "", "browser_read_page");
    assert_eq!(observation.turn_mode, turn_policy::TurnMode::Conversation);
}

#[test]
fn done_reaches_the_terminal_tool_without_a_language_gate() {
    let mut state = Reader {
        active: true,
        awaiting: true,
        turn_mode: turn_policy::TurnMode::Conversation,
        ..Reader::default()
    };
    refine_turn_mode(&mut state, "", "done");
    assert_eq!(state.turn_mode, turn_policy::TurnMode::Conversation);
    assert!(!guard_tool_call(
        &mut state,
        "done-id",
        "done",
        &serde_json::json!({"summary": "answered"}),
        telemetry::ActionTrace {
            action_id: 1,
            turn_id: 1,
        },
    ));
    assert!(!state.terminal_drain);
    assert!(state.active);
    assert!(state.awaiting);
    assert!(state.immediate_tool_responses.is_empty());
}

#[test]
fn cancellation_is_monotonic_and_scoped_to_one_job() {
    let first_cancel = Arc::new(AtomicBool::new(false));
    let mut first = Pending {
        id: Some("shared-id".to_string()),
        tool: Some("future_operation".to_string()),
        turn_id: Some(1),
        cancelled: false,
        cancel: Some(first_cancel.clone()),
    };
    assert!(first.request_cancel());
    assert!(first_cancel.load(Ordering::SeqCst));

    let second_cancel = Arc::new(AtomicBool::new(false));
    let second = Pending {
        id: Some("shared-id".to_string()),
        tool: Some("future_operation".to_string()),
        turn_id: Some(2),
        cancelled: false,
        cancel: Some(second_cancel.clone()),
    };
    assert!(!second_cancel.load(Ordering::SeqCst));
    assert!(!second.matches_result("shared-id", &first_cancel));
    assert!(second.matches_result("shared-id", &second_cancel));
}
