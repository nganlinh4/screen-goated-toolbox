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
    ActionComplete,
}

pub(super) fn apply_user_turn_policy(state: &mut Reader, _user_text: &str) -> bool {
    let superseded_generation = state.active || state.awaiting || state.pending.id.is_some();
    let cancelled_pending = state.pending.request_cancel();
    if cancelled_pending {
        mark_pending_interruption(state);
    }
    state.immediate_tool_responses.clear();
    state.turn_mode = turn_policy::TurnMode::Conversation;
    state.ignore_stale_boundary |= superseded_generation;
    state.active = true;
    state.awaiting = true;
    state.recovery_owed = true;
    state.think_start = Some(Instant::now());
    state.nudged = false;
    cancelled_pending
}

pub(super) fn mark_pending_interruption(state: &mut Reader) -> bool {
    let Some(tool) = state.pending.tool.clone() else {
        return false;
    };
    if !turn_policy::is_mutating_tool(&tool) {
        return false;
    }
    let newly_required = !state.reconciliation_required;
    state.reconciliation_required = true;
    if newly_required {
        state.turn_outcomes.record_interrupted_effect(&tool);
        telemetry::event(
            "interrupted_effect_reconciliation_required",
            "turn_policy",
            telemetry::Privacy::Safe,
            serde_json::json!({
                "tool": tool,
                "interrupted_turn_id": state.pending.turn_id,
            }),
        );
    }
    true
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
        turn_policy::TurnMode::Action => BoundaryOutcome::ActionComplete,
    }
}

pub(super) fn begin_terminal_drain(state: &mut Reader, accepted: bool, boundary_seen: bool) {
    state.carry_unfinished_goal = state.turn_mode == turn_policy::TurnMode::Action && !accepted;
    state.active = false;
    state.awaiting = false;
    state.recovery_owed = false;
    state.terminal_drain = true;
    state.terminal_accepted = accepted;
    state.terminal_boundary_seen = boundary_seen;
    state.terminal_dropped_events = 0;
    state.terminal_effectful_dropped_events = 0;
    state.terminal_response = super::terminal_drain::FinalResponseState::Closed;
    state.terminal_generation_complete = false;
    state.terminal_prior_turn_boundary_pending = false;
    state.terminal_activity_at = None;
    state.terminal_playback_cursor = None;
    state.ignore_stale_boundary = false;
    state.think_start = None;
    state.reasoning.clear();
    state.thinking.clear();
}

pub(super) fn retire_terminal_for_user_turn(state: &mut Reader) -> bool {
    let stale_boundary_possible = state.terminal_drain && !state.terminal_boundary_seen;
    state.carry_unfinished_goal |=
        state.turn_mode == turn_policy::TurnMode::Action && !state.terminal_accepted;
    state.terminal_drain = false;
    state.terminal_accepted = false;
    state.terminal_boundary_seen = false;
    state.terminal_response = super::terminal_drain::FinalResponseState::Closed;
    state.terminal_generation_complete = false;
    state.terminal_prior_turn_boundary_pending = false;
    state.terminal_activity_at = None;
    state.terminal_playback_cursor = None;
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
    // One local action owns the executor at a time. Keep later calls from the
    // same model generation pending until the owner's result has been delivered;
    // replying early would start a competing generation without that result.
    if state.pending.id.is_some() {
        let prior_turn = state
            .pending
            .turn_id
            .is_some_and(|pending_turn| pending_turn != action.turn_id);
        let status = if prior_turn {
            "blocked_prior_action_settling"
        } else {
            "blocked_tool_call_in_flight"
        };
        let code = if prior_turn {
            "prior_action_settling"
        } else {
            "tool_call_in_flight"
        };
        let message = if prior_turn {
            "The interrupted prior action is still settling. This call did not run. After its receipt arrives, inspect fresh state before any completion or mutation claim."
        } else {
            "Only one computer tool executes at a time. This call did not run. After the in-flight result arrives, retry it only if still needed against fresh state."
        };
        queue(
            state,
            id,
            name,
            serde_json::json!({
                "ok": false,
                "status": status,
                "executed": false,
                "effect_may_have_occurred": false,
                "error": {
                    "code": code,
                    "message": message,
                }
            }),
        );
        telemetry::typed_error(
            if prior_turn {
                "ERR_PRIOR_ACTION_SETTLING"
            } else {
                "ERR_CONCURRENT_TOOL_CALL"
            },
            "turn_policy",
            "held a not-run response until the owning in-flight tool result is delivered",
            serde_json::json!({
                "tool": name,
                "pending_turn_id": state.pending.turn_id,
                "requested_turn_id": action.turn_id,
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
                "effect_may_have_occurred": false,
                "effect_status": "proven_no_effect",
                "status": status,
                "error_code": code,
            }),
        );
        return true;
    }

    if state.reconciliation_required && (turn_policy::is_mutating_tool(name) || name == "done") {
        queue(
            state,
            id,
            name,
            serde_json::json!({
                "ok": false,
                "status": "blocked_interrupted_effect_reconciliation_required",
                "executed": false,
                "error": {
                    "code": "interrupted_effect_reconciliation_required",
                    "message": "An interrupted mutation still needs fresh observed state. Use a read or observation capability before any mutation or completion claim."
                }
            }),
        );
        state.awaiting = true;
        state.recovery_owed = true;
        state.think_start = Some(Instant::now());
        telemetry::typed_error(
            "ERR_TOOL_BEFORE_EFFECT_RECONCILIATION",
            "turn_policy",
            "blocked a mutation before fresh state reconciled an interrupted action",
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
                "effect_status": "proven_no_effect",
                "effect_may_have_occurred": false,
                "status": "blocked_interrupted_effect_reconciliation_required",
                "error_code": "interrupted_effect_reconciliation_required",
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
#[path = "reader_policy_tests.rs"]
mod tests;
