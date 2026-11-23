use super::{
    Reader, ResponseTransition, annotate_accepted_done_delivery, close_accepted_done,
    close_silent_terminal_blocker, close_terminal_blocker, delivery_status,
    handle_undelivered_result, immediate_tool_responses_ready, is_silent_terminal_blocker,
    is_terminal_blocker, mark_undelivered_result, receive_action_result,
    require_reconciliation_for_omitted_mutation, response_transition, result_status,
    settle_transport_receipt,
};
use crate::overlay::computer_control::runtime::reader::Pending;

#[test]
fn executor_disconnect_preserves_admitted_job_uncertainty_and_stop_race() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    };

    let stop = AtomicBool::new(false);
    let cancel = Arc::new(AtomicBool::new(false));
    let mut state = Reader {
        pending: Pending {
            id: Some("admitted".into()),
            tool: Some("run_command".into()),
            cancel: Some(Arc::clone(&cancel)),
            ..Pending::default()
        },
        ..Reader::default()
    };
    let (sender, receiver) = mpsc::channel();
    assert!(
        receive_action_result(&receiver, &mut state, &stop)
            .unwrap()
            .is_none()
    );
    drop(sender);
    let error = receive_action_result(&receiver, &mut state, &stop).unwrap_err();
    assert!(error.is::<super::ExecutorDisconnected>());
    assert!(error.to_string().contains("may have taken effect"));
    let mut exit_reason = "external_stop_flag";
    super::record_executor_disconnect_reason(&Err(error), &mut exit_reason);
    assert_eq!(exit_reason, "action_executor_disconnected");
    assert!(cancel.load(Ordering::SeqCst));
    assert!(state.reconciliation.required());
    assert_eq!(state.pending.id.as_deref(), Some("admitted"));
    stop.store(true, Ordering::SeqCst);
    assert!(
        receive_action_result(&receiver, &mut state, &stop)
            .unwrap()
            .is_none()
    );
}

#[test]
fn executor_drains_queued_results_before_reporting_disconnect() {
    use crate::overlay::computer_control::telemetry::ActionTrace;
    use std::sync::{Arc, atomic::AtomicBool, mpsc};

    let (sender, receiver) = mpsc::channel();
    sender
        .send((
            "job".into(),
            "observe".into(),
            serde_json::json!({}),
            None,
            Arc::new(AtomicBool::new(false)),
            ActionTrace {
                action_id: 1,
                turn_id: 1,
            },
            false,
        ))
        .unwrap();
    drop(sender);
    let mut state = Reader::default();
    let stop = AtomicBool::new(false);
    assert_eq!(
        receive_action_result(&receiver, &mut state, &stop)
            .unwrap()
            .unwrap()
            .0,
        "job"
    );
    assert!(
        receive_action_result(&receiver, &mut state, &stop)
            .unwrap_err()
            .is::<super::ExecutorDisconnected>()
    );
    assert!(!state.reconciliation.required());
}

#[test]
fn omitted_mutation_requires_reconciliation_without_trusting_integration_receipts() {
    let mut state = Reader::default();
    require_reconciliation_for_omitted_mutation(&mut state, "mcp__example", true);
    assert!(state.reconciliation.required());

    require_reconciliation_for_omitted_mutation(&mut state, "browser_eval", true);
    assert!(state.reconciliation.required());
}

#[test]
fn accepted_done_stays_terminal_when_its_response_write_fails() {
    for output_seen in [false, true] {
        let mut state = Reader {
            active: true,
            pending: Pending {
                id: Some("done-call".into()),
                tool: Some("done".into()),
                ..Pending::default()
            },
            ..Reader::default()
        };
        handle_undelivered_result(&mut state, "done", true, output_seen, None);
        assert!(state.pending.id.is_none());
        assert!(state.terminal_drain);
        assert!(state.terminal_accepted);
        assert_eq!(state.terminal_final_response_delivered, output_seen);
        super::super::terminal_drain::retire_for_connection_replacement(&mut state);
        assert!(state.terminal_drain);
        assert!(state.terminal_accepted);
        assert!(!state.active);
        assert!(!state.recovery_owed);
        assert!(!state.terminal_response.is_open());
    }
}

#[test]
fn failed_tool_response_write_retires_job_and_preserves_uncertainty() {
    let mut state = Reader {
        pending: Pending {
            id: Some("finished".into()),
            tool: Some("run_command".into()),
            ..Pending::default()
        },
        immediate_tool_responses: std::collections::VecDeque::from([(
            "old".into(),
            "done".into(),
            serde_json::json!({}),
        )]),
        ..Reader::default()
    };
    mark_undelivered_result(&mut state, "run_command");
    assert!(state.pending.id.is_none());
    assert!(state.immediate_tool_responses.is_empty());
    assert!(state.reconciliation.required());
    assert!(state.turn_outcomes.has_transport_uncertainty());
}

#[test]
fn cancellation_after_command_completion_keeps_receipt_and_requires_reconciliation() {
    use crate::overlay::computer_control::{executor, human_input::HumanProfile};
    use std::sync::{Arc, atomic::AtomicBool};

    let cancel = Arc::new(AtomicBool::new(false));
    let result = executor::execute_ex(
        "run_command",
        &serde_json::json!({"command": "Write-Output completed"}),
        &HumanProfile::instant(),
        &cancel,
    );
    assert_eq!(result["process_completed"], true);
    let mut state = Reader {
        pending: Pending {
            id: Some("owner".into()),
            tool: Some("run_command".into()),
            cancel: Some(cancel.clone()),
            ..Pending::default()
        },
        ..Reader::default()
    };
    assert!(state.pending.request_cancel());
    let (response, _) = super::super::effect_reporting::cancelled_after_dispatch(result, true);
    assert_eq!(response["action_result"]["process_completed"], true);
    assert_eq!(response["action_result"]["stdout"], "completed\r\n");
    assert_eq!(response["effect_may_have_occurred"], true);
    assert_eq!(
        delivery_status(false),
        ("effect_unknown_response_not_delivered", true)
    );
    settle_transport_receipt(&mut state, "owner", &cancel, "run_command", false);
    assert!(state.pending.cancelled);
    assert!(state.reconciliation.required());
}

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
fn delivery_reports_only_host_owned_dispatch_certainty() {
    assert_eq!(
        delivery_status(false),
        ("effect_unknown_response_not_delivered", true)
    );
    assert_eq!(
        delivery_status(true),
        (
            "requested_action_not_dispatched_response_not_delivered",
            true
        )
    );
}

#[test]
fn only_matching_trusted_pre_dispatch_receipt_clears_interruption() {
    use std::sync::{Arc, atomic::AtomicBool};
    let token = Arc::new(AtomicBool::new(false));
    let mut state = Reader {
        pending: Pending {
            id: Some("owner".into()),
            tool: Some("edit_text_file".into()),
            cancel: Some(token.clone()),
            ..Pending::default()
        },
        ..Reader::default()
    };
    state.reconciliation.require_pending(&state.pending);
    state.turn_outcomes.record_transport_interruption_for(
        "edit_text_file",
        super::super::reconciliation::JobIdentity::from_pending(&state.pending),
    );

    settle_transport_receipt(&mut state, "other", &token, "edit_text_file", true);
    assert!(state.reconciliation.required());
    settle_transport_receipt(&mut state, "owner", &token, "edit_text_file", true);
    assert!(!state.reconciliation.required());
    assert!(!state.turn_outcomes.has_transport_uncertainty());
}
