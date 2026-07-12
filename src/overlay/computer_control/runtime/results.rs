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
        reconnect_for_activation = name == "app_integration_status"
            && super::session_control::activation_pending(&response);
        record_observation(state, &name, &response);
        record_tool_result(state, &name, &response);
        if response_ok && answer_evidence_tool(&name) {
            state.speech_gate.defer_until_boundary(false);
            telemetry::event_for_action(
                "answer_evidence_ready",
                "speech",
                Privacy::Safe,
                action,
                serde_json::json!({"tool": name.clone()}),
            );
        }
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
        if let Some((b64, frame_id)) = frame
            && super::frames::send_action_frame(socket, &b64, frame_id, action).is_ok()
        {
            state.source_frame_id = Some(frame_id);
        }
        if name == "done" && response_ok {
            overlay::push_log("[done] goal reached".to_string());
            overlay::set_orb_done();
            emit_turn_summary(state, "done");
            state.active = false;
            state.awaiting = false;
            state.awaiting_done_boundary = true;
        } else if reconnect_for_activation {
            state.awaiting = false;
        } else {
            state.awaiting = true;
            state.think_start = Some(Instant::now());
        }
    }
    state.pending = Pending::default();
    state.nudged = false;
    overlay::set_status("ready - speak a command");
    Ok(Some(reconnect_for_activation))
}

fn answer_evidence_tool(name: &str) -> bool {
    matches!(
        name,
        "look"
            | "observe"
            | "research_web"
            | "browser_read_page"
            | "browser_extract_page"
            | "system_query"
            | "list_files"
            | "read_clipboard"
            | "list_windows"
            | "search_memory"
            | "open_memory"
    )
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
    use super::answer_evidence_tool;

    #[test]
    fn answer_gate_opens_for_evidence_not_status_or_navigation() {
        assert!(answer_evidence_tool("look"));
        assert!(answer_evidence_tool("browser_read_page"));
        assert!(!answer_evidence_tool("browser_status"));
        assert!(!answer_evidence_tool("open_url"));
        assert!(!answer_evidence_tool("see_whole_screen"));
    }
}
