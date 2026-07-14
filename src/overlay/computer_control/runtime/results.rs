//! Delivery and state transitions for completed executor jobs.

use super::*;

pub(super) fn poll_action_result(
    socket: &mut Sock,
    receiver: &mpsc::Receiver<Done>,
    state: &mut Reader,
) -> anyhow::Result<Option<bool>> {
    let Ok((id, name, response, frame, result_cancel, action)) = receiver.try_recv() else {
        return Ok(None);
    };
    if !state.pending.matches_result(&id, &result_cancel) {
        let delivery = delivery_status(&response);
        telemetry::typed_error(
            "ERR_STALE_ACTION_RESULT",
            "runtime",
            "discarded a result that did not belong to the current pending job",
            serde_json::json!({
                "tool_call_id": id,
                "tool": name,
                "current_pending_id": state.pending.id.clone(),
            }),
        );
        telemetry::event_for_action(
            "action_result_dropped",
            "runtime",
            Privacy::Safe,
            action,
            serde_json::json!({
                "tool_call_id": id,
                "tool": name,
                "reason": "stale_job_identity",
                "delivery_status": delivery.0,
                "effect_may_have_occurred": delivery.1,
            }),
        );
        return Ok(None);
    }

    let mut reconnect_for_activation = false;
    if state.pending.cancelled {
        let delivery = delivery_status(&response);
        overlay::push_log(if delivery.1 {
            "[~] action effect completed before interruption; result dropped".to_string()
        } else {
            "[~] action cancelled before a confirmed effect; result dropped".to_string()
        });
        telemetry::event_for_action(
            "action_result_dropped",
            "runtime",
            Privacy::Safe,
            action,
            serde_json::json!({
                "tool_call_id": id,
                "tool": name,
                "reason": "cancelled_or_superseded",
                "delivery_status": delivery.0,
                "effect_may_have_occurred": delivery.1,
                "execution_ok": response.get("execution_ok").or_else(|| response.get("ok")),
                "injection": super::super::executor::input_injection(&response),
            }),
        );
    } else {
        let response_ok = response.get("ok").and_then(Value::as_bool).unwrap_or(false);
        let terminal_blocker = is_terminal_blocker(&response);
        reconnect_for_activation = name == "app_integration_status"
            && super::session_control::activation_pending(&response);
        let delivered_response = response.clone();
        let response_send = send(socket, tool_response(&id, &name, response));
        telemetry::event_for_action(
            "tool_response_sent",
            "runtime",
            Privacy::Safe,
            action,
            serde_json::json!({
                "tool_call_id": id,
                "tool": name,
                "transport_ok": response_send.is_ok(),
                "semantic_ok": response_ok,
                "error": response_send.as_ref().err().map(ToString::to_string),
            }),
        );
        response_send?;
        record_observation(state, &name, &delivered_response);
        record_tool_result(state, &name);
        state
            .turn_outcomes
            .record_delivered(&name, &delivered_response);
        if let Some((b64, source)) = frame
            && super::frames::send_action_frame(socket, &b64, &source, action).is_ok()
        {
            state.source_frame = Some(source);
        }
        if name == "done" && response_ok {
            overlay::push_log("[done] goal reached".to_string());
            overlay::set_orb_done();
            emit_turn_summary(state, "done");
            super::terminal_drain::begin_final_response(state, true);
        } else if terminal_blocker {
            close_terminal_blocker(state);
        } else {
            state.awaiting = true;
            state.recovery_owed = true;
            state.think_start = Some(Instant::now());
        }
    }
    state.pending = Pending::default();
    if !state.terminal_response.is_open() {
        state.pending_tool_boundary_seen = false;
    }
    state.nudged = false;
    overlay::set_status(result_status(state, reconnect_for_activation));
    Ok(Some(reconnect_for_activation))
}

pub(super) fn send_immediate_tool_responses(
    socket: &mut Sock,
    state: &mut Reader,
) -> anyhow::Result<bool> {
    let mut sent = false;
    while let Some((id, name, response)) = state.immediate_tool_responses.pop_front() {
        let delivered_response = response.clone();
        send(socket, tool_response(&id, &name, response))?;
        state
            .turn_outcomes
            .record_delivered(&name, &delivered_response);
        telemetry::event(
            "immediate_tool_response_sent",
            "turn_policy",
            Privacy::Safe,
            serde_json::json!({
                "tool_call_id": id,
                "tool": name,
                "turn_mode": state.turn_mode.as_str(),
            }),
        );
        sent = true;
    }
    Ok(sent)
}

fn is_terminal_blocker(response: &Value) -> bool {
    response
        .get("terminal_blocker")
        .or_else(|| response.pointer("/action_result/terminal_blocker"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn close_terminal_blocker(state: &mut Reader) {
    emit_turn_summary(state, "terminal_blocker");
    super::terminal_drain::begin_final_response(state, false);
    overlay::set_orb_resting();
}

fn result_status(state: &Reader, reconnect_for_activation: bool) -> &'static str {
    if state.awaiting {
        "working..."
    } else if state.terminal_drain && !state.terminal_accepted {
        "blocked - speak a new command"
    } else if reconnect_for_activation {
        "waiting to refresh capabilities..."
    } else {
        "ready - speak a command"
    }
}

fn delivery_status(response: &Value) -> (&'static str, bool) {
    let injected = super::super::executor::input_injection(response)
        .and_then(|value| value.get("fully_inserted"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let execution_ok = response
        .get("execution_ok")
        .or_else(|| response.get("ok"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if injected || execution_ok {
        ("effect_completed_response_not_delivered", true)
    } else if response.get("cancelled").and_then(Value::as_bool) == Some(true) {
        ("cancelled_before_confirmed_effect", false)
    } else {
        ("effect_unknown_response_not_delivered", true)
    }
}

#[cfg(test)]
mod tests {
    use super::{Reader, close_terminal_blocker, is_terminal_blocker, result_status};

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
        close_terminal_blocker(&mut state);
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
}
