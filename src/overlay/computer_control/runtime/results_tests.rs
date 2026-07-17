use super::{
    Reader, ResponseTransition, annotate_accepted_done_delivery, close_accepted_done,
    close_silent_terminal_blocker, close_terminal_blocker, delivery_status,
    immediate_tool_responses_ready, is_silent_terminal_blocker, is_terminal_blocker,
    reconcile_from_observation, response_transition, result_status, settle_transport_receipt,
};
use crate::overlay::computer_control::runtime::reader::Pending;

#[test]
fn policy_responses_wait_for_the_in_flight_result() {
    let mut state = Reader {
        immediate_tool_responses: std::collections::VecDeque::from([(
            "later".to_string(),
            "future_tool".to_string(),
            serde_json::json!({"ok": false}),
        )]),
        ..Reader::default()
    };
    assert!(immediate_tool_responses_ready(&state));

    state.pending.id = Some("owner".to_string());
    assert!(!immediate_tool_responses_ready(&state));

    state.pending = Pending::default();
    assert!(immediate_tool_responses_ready(&state));
}

#[test]
fn terminal_blocker_allows_one_explanation_without_accepting_completion() {
    assert!(is_terminal_blocker(
        &serde_json::json!({"terminal_blocker": true})
    ));
    assert!(is_terminal_blocker(&serde_json::json!({
        "action_result": {"terminal_blocker": true}
    })));
    let mut state = Reader {
        active: true,
        awaiting: true,
        recovery_owed: true,
        ..Reader::default()
    };
    close_terminal_blocker(&mut state, None, false);
    assert!(state.terminal_drain);
    assert!(!state.terminal_accepted);
    assert!(state.active);
    assert!(state.awaiting);
    assert!(!state.recovery_owed);
    assert!(state.terminal_response.is_open());
    assert_eq!(result_status(&state, false), "working...");

    let working = Reader {
        awaiting: true,
        ..Reader::default()
    };
    assert_eq!(result_status(&working, false), "working...");
}

#[test]
fn terminal_blocker_takes_precedence_over_an_unsuccessful_done_result() {
    assert_eq!(
        response_transition("done", false, true, false),
        ResponseTransition::TerminalBlocker
    );
    assert_eq!(
        response_transition("done", false, false, false),
        ResponseTransition::FailedDone
    );
    assert_eq!(
        response_transition("done", true, true, true),
        ResponseTransition::AcceptedDone
    );
}

#[test]
fn silent_accepted_done_requests_one_final_sentence() {
    let mut response = serde_json::json!({
        "ok": true,
        "summary": "Finished the requested work."
    });
    annotate_accepted_done_delivery("done", true, false, &mut response);
    assert_eq!(response["final_response_required"], true);
    assert!(
        response["instruction"]
            .as_str()
            .is_some_and(|text| text.contains("exactly once"))
    );

    let mut already_spoken = response.clone();
    already_spoken
        .as_object_mut()
        .unwrap()
        .remove("instruction");
    already_spoken
        .as_object_mut()
        .unwrap()
        .remove("final_response_required");
    annotate_accepted_done_delivery("done", true, true, &mut already_spoken);
    assert!(already_spoken.get("instruction").is_none());
    assert!(already_spoken.get("final_response_required").is_none());
}

#[test]
fn accepted_done_never_opens_a_second_generation_after_pre_tool_speech() {
    let mut already_spoken = Reader {
        active: true,
        awaiting: true,
        reply: "Finished once.".to_string(),
        ..Reader::default()
    };
    close_accepted_done(&mut already_spoken, None, true);
    assert!(already_spoken.terminal_drain);
    assert!(already_spoken.terminal_accepted);
    assert!(already_spoken.terminal_final_response_delivered);
    assert!(!already_spoken.terminal_response.is_open());
    assert!(!already_spoken.active);
    assert!(!already_spoken.awaiting);

    let mut silent = Reader {
        active: true,
        awaiting: true,
        ..Reader::default()
    };
    close_accepted_done(&mut silent, None, false);
    assert!(silent.terminal_drain);
    assert!(silent.terminal_accepted);
    assert!(!silent.terminal_final_response_delivered);
    assert!(silent.terminal_response.is_open());
}

#[test]
fn duplicate_completion_blocker_ends_without_opening_more_speech() {
    let response = serde_json::json!({
        "terminal_blocker": true,
        "silent_terminal_blocker": true,
    });
    assert!(is_terminal_blocker(&response));
    assert!(is_silent_terminal_blocker(&response));
    assert_eq!(
        response_transition("done", false, true, true),
        ResponseTransition::SilentTerminalBlocker
    );
    let mut state = Reader {
        active: true,
        awaiting: true,
        ..Reader::default()
    };
    close_silent_terminal_blocker(&mut state);
    assert!(state.terminal_drain);
    assert!(!state.terminal_accepted);
    assert!(!state.terminal_response.is_open());
    assert!(!state.active);
    assert!(!state.awaiting);
    assert_eq!(
        result_status(&state, false),
        "blocked - speak a new command"
    );
}

#[test]
fn delivery_keeps_nested_effect_certainty() {
    assert_eq!(
        delivery_status(&serde_json::json!({
            "cancelled": true,
            "action_result": {"effect_verified": true},
        })),
        ("effect_completed_response_not_delivered", true)
    );
    assert_eq!(
        delivery_status(&serde_json::json!({
            "cancelled": true,
            "action_result": {"effect_may_have_occurred": true},
        })),
        ("effect_may_have_occurred_response_not_delivered", true)
    );
    assert_eq!(
        delivery_status(&serde_json::json!({
            "cancelled": true,
            "action_result": {"effect_may_have_occurred": false},
        })),
        ("proven_no_effect_response_not_delivered", false)
    );
}

#[test]
fn interrupted_effect_reconciliation_needs_no_effect_or_fresh_observation() {
    let mut state = Reader {
        reconciliation_required: true,
        ..Reader::default()
    };
    state
        .turn_outcomes
        .record_transport_interruption("edit_text_file");

    settle_transport_receipt(
        &mut state,
        "edit_text_file",
        &serde_json::json!({"effect_may_have_occurred": true}),
    );
    assert!(state.reconciliation_required);
    reconcile_from_observation(&mut state, "wait", true);
    assert!(state.reconciliation_required);
    reconcile_from_observation(&mut state, "observe", false);
    assert!(state.reconciliation_required);
    reconcile_from_observation(&mut state, "observe", true);
    assert!(!state.reconciliation_required);
    assert!(!state.turn_outcomes.has_transport_uncertainty());

    state.reconciliation_required = true;
    state
        .turn_outcomes
        .record_transport_interruption("edit_text_file");
    settle_transport_receipt(
        &mut state,
        "edit_text_file",
        &serde_json::json!({"effect_may_have_occurred": false}),
    );
    assert!(!state.reconciliation_required);
    assert!(!state.turn_outcomes.has_transport_uncertainty());
}
