//! Deterministic user-turn lifecycle and capability gate for the live reader.

use std::time::Instant;

use serde_json::Value;

use super::super::protocol::ServerEvent;
use super::super::telemetry;
use super::super::turn_policy;
use super::reader::Reader;

pub(super) type ImmediateToolResponse = (String, String, Value);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BoundaryOutcome {
    PendingTool,
    AlreadyIdle,
    ConversationComplete,
    ActionUnverified,
}

pub(super) fn apply_user_turn_policy(state: &mut Reader, _user_text: &str) -> bool {
    let superseded_generation = state.active || state.awaiting || state.pending.id.is_some();
    let cancelled_pending = state.pending.request_cancel();
    state.turn_mode = turn_policy::TurnMode::Conversation;
    state.ignore_stale_boundary |= superseded_generation;
    state.active = true;
    state.awaiting = true;
    state.recovery_owed = true;
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
pub(super) fn finish_at_model_boundary(state: &mut Reader) -> BoundaryOutcome {
    if state.pending.id.is_some() {
        return BoundaryOutcome::PendingTool;
    }
    state.think_start = None;
    state.awaiting = false;
    if !std::mem::replace(&mut state.active, false) {
        return BoundaryOutcome::AlreadyIdle;
    }
    match state.turn_mode {
        turn_policy::TurnMode::Conversation => BoundaryOutcome::ConversationComplete,
        turn_policy::TurnMode::Action => BoundaryOutcome::ActionUnverified,
    }
}

pub(super) fn begin_terminal_drain(state: &mut Reader, accepted: bool, boundary_seen: bool) {
    state.input_transcript.reset();
    state.active = false;
    state.awaiting = false;
    state.recovery_owed = false;
    state.terminal_drain = true;
    state.terminal_accepted = accepted;
    state.terminal_boundary_seen = boundary_seen;
    state.terminal_dropped_events = 0;
    state.terminal_response = super::terminal_drain::FinalResponseState::Closed;
    state.terminal_activity_at = None;
    state.ignore_stale_boundary = false;
    state.think_start = None;
    state.reasoning.clear();
    state.thinking.clear();
}

pub(super) fn retire_terminal_for_user_turn(state: &mut Reader) -> bool {
    let stale_boundary_possible = state.terminal_drain && !state.terminal_boundary_seen;
    state.terminal_drain = false;
    state.terminal_accepted = false;
    state.terminal_boundary_seen = false;
    state.terminal_response = super::terminal_drain::FinalResponseState::Closed;
    state.terminal_activity_at = None;
    state.ignore_stale_boundary = stale_boundary_possible;
    stale_boundary_possible
}

pub(super) fn record_generation_progress(state: &mut Reader) {
    state.ignore_stale_boundary = false;
    state.recovery_owed = false;
}

pub(super) fn is_real_generation_progress(event: &ServerEvent) -> bool {
    match event {
        ServerEvent::Audio(samples) => !samples.is_empty(),
        ServerEvent::OutputTranscript(text)
        | ServerEvent::Thought(text)
        | ServerEvent::ModelText(text) => !text.trim().is_empty(),
        ServerEvent::ToolCall { .. } => true,
        _ => false,
    }
}

pub(super) fn recovery_due(state: &Reader) -> bool {
    state.awaiting && state.recovery_owed && state.pending.id.is_none()
}

pub(super) fn consume_stale_boundary(state: &mut Reader) -> bool {
    std::mem::take(&mut state.ignore_stale_boundary)
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
                "status": "blocked_previous_action_halting",
                "executed": false,
                "error": {
                    "code": "previous_action_halting",
                    "message": "The previous action is still halting. Do not act yet; wait for the user or retry after it has stopped."
                }
            }),
        );
        state.awaiting = true;
        state.recovery_owed = true;
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
                tool: Some("future_operation".to_string()),
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
        assert_eq!(state.turn_mode, turn_policy::TurnMode::Conversation);
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
            BoundaryOutcome::ActionUnverified
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
    fn done_always_reaches_the_independent_verifier() {
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
            cancelled: false,
            cancel: Some(first_cancel.clone()),
        };
        assert!(first.request_cancel());
        assert!(first_cancel.load(Ordering::SeqCst));

        let second_cancel = Arc::new(AtomicBool::new(false));
        let second = Pending {
            id: Some("shared-id".to_string()),
            tool: Some("future_operation".to_string()),
            cancelled: false,
            cancel: Some(second_cancel.clone()),
        };
        assert!(!second_cancel.load(Ordering::SeqCst));
        assert!(!second.matches_result("shared-id", &first_cancel));
        assert!(second.matches_result("shared-id", &second_cancel));
    }
}
