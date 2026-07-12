//! Deterministic user-turn lifecycle and capability gate for the live reader.

use std::time::Instant;

use serde_json::Value;

use super::super::overlay;
use super::super::telemetry;
use super::super::turn_policy;
use super::reader::{Reader, emit_turn_summary};

pub(super) type ImmediateToolResponse = (String, String, Value);

pub(super) fn apply_user_turn_policy(state: &mut Reader, _user_text: &str) -> bool {
    let cancelled_pending = state.pending.request_cancel();
    state.turn_mode = turn_policy::TurnMode::Conversation;
    state.speech_gate.defer_until_boundary(false);
    state.control_nudge = None;
    state.active = true;
    state.awaiting = true;
    state.think_start = Some(Instant::now());
    state.nudged = false;
    cancelled_pending
}

pub(super) fn refine_turn_mode(state: &mut Reader, _intent: &str, tool: &str) {
    if turn_policy::is_mutating_tool(tool) {
        state.turn_mode = turn_policy::TurnMode::Action;
    }
}

/// Treat the model's own turn boundary as terminal once no tool is in flight.
/// A boundary is not permission for the harness to manufacture another user
/// turn: doing that makes a completed task repeatedly talk and keep exploring.
pub(super) fn finish_at_model_boundary(state: &mut Reader) -> bool {
    if state.pending.id.is_some() {
        return false;
    }
    state.think_start = None;
    state.control_nudge = None;
    state.awaiting = false;
    std::mem::replace(&mut state.active, false)
}

/// Return true when policy handled the call locally, so the caller must not send
/// it to the executor.
pub(super) fn guard_tool_call(
    state: &mut Reader,
    id: &str,
    name: &str,
    _args: &Value,
    action: telemetry::ActionTrace,
) -> bool {
    // Never overwrite an older pending id while its cancelled job unwinds.
    if state.pending.id.is_some() {
        queue(
            state,
            id,
            name,
            serde_json::json!({
                "ok": false,
                "error": {
                    "code": "previous_action_halting",
                    "message": "The previous action is still halting. Do not act yet; wait for the user or retry after it has stopped."
                }
            }),
        );
        state.awaiting = true;
        state.think_start = Some(Instant::now());
        telemetry::typed_error(
            "ERR_TOOL_WHILE_PREVIOUS_HALTING",
            "turn_policy",
            "blocked a tool call while the prior action was still halting",
            serde_json::json!({"tool": name}),
        );
        telemetry::event_for_action(
            "action_outcome",
            "turn_policy",
            telemetry::Privacy::Safe,
            action,
            serde_json::json!({
                "tool_call_id": id,
                "requested_tool": name,
                "executed": false,
                "status": "blocked_previous_action_halting",
            }),
        );
        return true;
    }

    // Answers and observations have no desktop postcondition to verify.
    if name == "done" && !turn_policy::needs_visual_done(state.turn_mode) {
        queue(
            state,
            id,
            name,
            serde_json::json!({
                "ok": true,
                "verification": "not_applicable",
                "verdict": "No state-changing capability ran, so desktop verification is not applicable."
            }),
        );
        state.active = false;
        state.awaiting = false;
        state.awaiting_done_boundary = true;
        state.think_start = None;
        state.control_nudge = None;
        overlay::push_log("[done] answer completed (visual check not applicable)".to_string());
        overlay::set_orb_done();
        emit_turn_summary(state, "answered");
        telemetry::event_for_action(
            "action_outcome",
            "turn_policy",
            telemetry::Privacy::Safe,
            action,
            serde_json::json!({
                "tool_call_id": id,
                "requested_tool": name,
                "executed": false,
                "status": "completed_without_desktop_action",
                "ok": true,
            }),
        );
        return true;
    }

    false
}

fn queue(state: &mut Reader, id: &str, name: &str, response: Value) {
    state
        .immediate_tool_responses
        .push_back((id.to_string(), name.to_string(), response));
}

#[cfg(test)]
mod tests {
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
                cancelled: false,
                cancel: Some(cancel.clone()),
            },
            active: true,
            awaiting: true,
            control_nudge: Some("continue".to_string()),
            turn_mode: turn_policy::TurnMode::Action,
            ..Reader::default()
        };

        assert!(apply_user_turn_policy(&mut state, "new turn"));
        assert!(cancel.load(Ordering::SeqCst));
        assert!(state.pending.cancelled);
        assert!(state.active);
        assert!(state.control_nudge.is_none());
        assert_eq!(state.turn_mode, turn_policy::TurnMode::Conversation);
    }

    #[test]
    fn a_model_boundary_finishes_answers_and_actions_without_self_reviving() {
        let mut answer = Reader {
            active: true,
            turn_mode: turn_policy::TurnMode::Conversation,
            think_start: Some(Instant::now()),
            control_nudge: Some("continue acting".to_string()),
            ..Reader::default()
        };
        assert!(finish_at_model_boundary(&mut answer));
        assert!(!answer.active);
        assert!(answer.think_start.is_none());
        assert!(answer.control_nudge.is_none());

        let mut action = Reader {
            active: true,
            turn_mode: turn_policy::TurnMode::Action,
            think_start: Some(Instant::now()),
            ..Reader::default()
        };
        assert!(finish_at_model_boundary(&mut action));
        assert!(!action.active);
        assert!(action.think_start.is_none());
        assert!(action.control_nudge.is_none());

        let mut in_flight = Reader {
            pending: Pending {
                id: Some("tool-call".to_string()),
                ..Pending::default()
            },
            active: true,
            turn_mode: turn_policy::TurnMode::Action,
            ..Reader::default()
        };
        assert!(!finish_at_model_boundary(&mut in_flight));
        assert!(in_flight.active);
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
    fn cancellation_is_monotonic_and_scoped_to_one_job() {
        let first_cancel = Arc::new(AtomicBool::new(false));
        let mut first = Pending {
            id: Some("shared-id".to_string()),
            cancelled: false,
            cancel: Some(first_cancel.clone()),
        };
        assert!(first.request_cancel());
        assert!(first_cancel.load(Ordering::SeqCst));

        let second_cancel = Arc::new(AtomicBool::new(false));
        let second = Pending {
            id: Some("shared-id".to_string()),
            cancelled: false,
            cancel: Some(second_cancel.clone()),
        };
        assert!(!second_cancel.load(Ordering::SeqCst));
        assert!(!second.matches_result("shared-id", &first_cancel));
        assert!(second.matches_result("shared-id", &second_cancel));
    }
}
