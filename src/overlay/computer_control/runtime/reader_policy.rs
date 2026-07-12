//! Deterministic user-turn lifecycle and capability gate for the live reader.

use std::time::Instant;

use serde_json::Value;

use super::super::overlay;
use super::super::telemetry;
use super::super::turn_policy;
use super::reader::{Reader, emit_turn_summary};

pub(super) type ImmediateToolResponse = (String, String, Value);

pub(super) fn apply_user_turn_policy(state: &mut Reader, user_text: &str) -> bool {
    let mode = turn_policy::turn_mode(user_text, "");
    let cancelled_pending = state.pending.request_cancel();
    let follows_action_offer = turn_policy::is_affirmative_followup(user_text)
        && state.history.last().is_some_and(|line| {
            line.starts_with("Assistant:") && (line.contains('?') || line.contains('？'))
        });

    if turn_policy::explicitly_authorizes_control(user_text) {
        state.control_revoked = false;
    }
    if mode == turn_policy::TurnMode::Stopped {
        state.control_revoked = true;
    }

    state.turn_mode = mode;
    state
        .speech_gate
        .defer_until_boundary(mode == turn_policy::TurnMode::ReadOnly);
    state.intent_may_authorize_action = matches!(
        mode,
        turn_policy::TurnMode::Conversation | turn_policy::TurnMode::ReadOnly
    ) && turn_policy::intent_may_authorize_control(user_text)
        && !state.control_revoked
        || follows_action_offer && !state.control_revoked;
    state.control_nudge = None;
    state.active = mode != turn_policy::TurnMode::Stopped;
    state.awaiting = true;
    state.think_start = (mode != turn_policy::TurnMode::Stopped).then(Instant::now);
    state.nudged = mode == turn_policy::TurnMode::Stopped;
    cancelled_pending
}

pub(super) fn refine_turn_mode(state: &mut Reader, intent: &str, tool: &str) {
    if state.turn_mode != turn_policy::TurnMode::Conversation {
        return;
    }
    let inferred_mode = turn_policy::turn_mode(&state.last_user_text, intent);
    let intent_class = turn_policy::classify("", intent);
    let model_refinement =
        turn_policy::substantive_turn_allows_action_refinement(&state.last_user_text, tool);
    if (state.intent_may_authorize_action || model_refinement)
        && (model_refinement
            || inferred_mode == turn_policy::TurnMode::Action
            || matches!(
                intent_class,
                turn_policy::TaskClass::DesktopAction
                    | turn_policy::TaskClass::BrowserAction
                    | turn_policy::TaskClass::Setup
            ))
        && !state.control_revoked
    {
        state.turn_mode = turn_policy::TurnMode::Action;
        state.intent_may_authorize_action = false;
    }
}

/// Close a non-action turn without scheduling action recovery. Returns whether
/// this call transitioned an active turn to idle.
pub(super) fn finish_without_action_recovery(state: &mut Reader) -> bool {
    if state.turn_mode.needs_action_completion() && !state.control_revoked {
        return false;
    }
    state.think_start = None;
    state.control_nudge = None;
    std::mem::replace(&mut state.active, false)
}

/// Return true when policy handled the call locally, so the caller must not send
/// it to the executor.
pub(super) fn guard_tool_call(
    state: &mut Reader,
    id: &str,
    name: &str,
    args: &Value,
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
                "verdict": "The conversational or read-only turn is complete; no desktop change requires visual verification."
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

    let access = turn_policy::call_access(
        state.turn_mode,
        state.control_revoked,
        name,
        args,
        &state.last_user_text,
    );
    if access == turn_policy::ToolAccess::Allow {
        return false;
    }

    let reason = access.reason();
    queue(
        state,
        id,
        name,
        serde_json::json!({
            "ok": false,
            "error": {"code": "control_not_authorized", "message": reason},
            "policy": {
                "turn_mode": state.turn_mode.as_str(),
                "control_revoked": state.control_revoked,
            },
            "instruction": "Do not exceed the user's requested action scope. If they asked only to type/fill, leave the content unsent and finish."
        }),
    );
    state.control_nudge = None;
    state.awaiting = true;
    state.think_start = Some(Instant::now());
    overlay::set_status("control blocked by user intent");
    overlay::push_log(format!("[policy] blocked {name}: {reason}"));
    telemetry::typed_error(
        "ERR_TOOL_NOT_AUTHORIZED",
        "turn_policy",
        reason,
        serde_json::json!({
            "tool": name,
            "turn_mode": state.turn_mode.as_str(),
            "control_revoked": state.control_revoked,
        }),
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
            "status": "blocked_not_authorized",
            "turn_mode": state.turn_mode.as_str(),
            "control_revoked": state.control_revoked,
        }),
    );
    true
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
    fn stop_latches_control_clears_recovery_and_cancels_pending_work() {
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

        assert!(apply_user_turn_policy(&mut state, "Please stop now"));
        assert!(cancel.load(Ordering::SeqCst));
        assert!(state.pending.cancelled);
        assert!(state.control_revoked);
        assert!(!state.active);
        assert!(state.control_nudge.is_none());
        assert_eq!(state.turn_mode, turn_policy::TurnMode::Stopped);
    }

    #[test]
    fn a_later_explicit_action_reopens_control_but_advice_does_not() {
        let mut state = Reader {
            control_revoked: true,
            ..Reader::default()
        };

        apply_user_turn_policy(&mut state, "Just explain what I should do");
        assert!(state.control_revoked);
        assert_eq!(state.turn_mode, turn_policy::TurnMode::ReadOnly);

        apply_user_turn_policy(&mut state, "Open the preferences window");
        assert!(!state.control_revoked);
        assert_eq!(state.turn_mode, turn_policy::TurnMode::Action);
    }

    #[test]
    fn a_spoken_answer_finishes_without_action_recovery() {
        let mut answer = Reader {
            active: true,
            turn_mode: turn_policy::TurnMode::ReadOnly,
            think_start: Some(Instant::now()),
            control_nudge: Some("continue acting".to_string()),
            ..Reader::default()
        };
        assert!(finish_without_action_recovery(&mut answer));
        assert!(!answer.active);
        assert!(answer.think_start.is_none());
        assert!(answer.control_nudge.is_none());

        let mut action = Reader {
            active: true,
            turn_mode: turn_policy::TurnMode::Action,
            think_start: Some(Instant::now()),
            ..Reader::default()
        };
        assert!(!finish_without_action_recovery(&mut action));
        assert!(action.active);
        assert!(action.think_start.is_some());
    }

    #[test]
    fn affirmative_followup_to_an_action_offer_can_refine_to_action() {
        let mut state = Reader {
            history: vec!["Assistant: Would you like me to set that up?".to_string()],
            ..Reader::default()
        };
        apply_user_turn_policy(&mut state, "Được");
        assert!(state.intent_may_authorize_action);
        refine_turn_mode(&mut state, "Set up browser control", "browser_setup");
        assert_eq!(state.turn_mode, turn_policy::TurnMode::Action);

        let mut no_offer = Reader::default();
        apply_user_turn_policy(&mut no_offer, "Được");
        assert!(!no_offer.intent_may_authorize_action);
    }

    #[test]
    fn substantive_turn_can_refine_to_action_without_language_specific_parsing() {
        let request = "Vui lòng thực hiện thao tác mà tôi vừa yêu cầu";
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

        let mut ambiguous = Reader {
            last_user_text: "Right there".to_string(),
            ..Reader::default()
        };
        apply_user_turn_policy(&mut ambiguous, "Right there");
        refine_turn_mode(
            &mut ambiguous,
            "Click at the indicated location",
            "click_here",
        );
        assert_eq!(ambiguous.turn_mode, turn_policy::TurnMode::Conversation);
    }

    #[test]
    fn read_only_turn_is_not_promoted_by_supporting_navigation() {
        let request = "Search the internet and explain the answer";
        let mut state = Reader {
            last_user_text: request.to_string(),
            turn_mode: turn_policy::TurnMode::ReadOnly,
            ..Reader::default()
        };
        refine_turn_mode(&mut state, "Open a search result", "open_url");
        assert_eq!(state.turn_mode, turn_policy::TurnMode::ReadOnly);
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
