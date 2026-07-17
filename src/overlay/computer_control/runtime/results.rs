//! Delivery and state transitions for completed executor jobs.

use super::*;

pub(super) fn poll_action_result(
    socket: &mut Sock,
    receiver: &mpsc::Receiver<Done>,
    state: &mut Reader,
    sink: Option<&AudioSink>,
) -> anyhow::Result<Option<bool>> {
    let Ok((id, name, mut response, frame, result_cancel, action)) = receiver.try_recv() else {
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
        settle_transport_receipt(state, &name, &response);
        let message = match delivery.0 {
            "effect_completed_response_not_delivered" => {
                "[~] action effect completed before interruption; result dropped"
            }
            "proven_no_effect_response_not_delivered" => {
                "[~] action cancelled before any effect; result dropped"
            }
            _ => "[~] action may have affected state before interruption; result dropped",
        };
        overlay::push_log(message.to_string());
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
        let silent_terminal_blocker = is_silent_terminal_blocker(&response);
        let completion_generation_output_seen =
            state.pending_tool_output_seen || state.generation_output_seen;
        annotate_accepted_done_delivery(
            &name,
            response_ok,
            completion_generation_output_seen,
            &mut response,
        );
        reconnect_for_activation = name == "app_integration_status"
            && super::session_control::activation_pending(&response);
        let delivered_response = response.clone();
        let wire_response = tool_response(&id, &name, response);
        let result_byte_count =
            serde_json::to_vec(&delivered_response).map_or(0, |bytes| bytes.len());
        let response_byte_count = serde_json::to_vec(&wire_response).map_or(0, |bytes| bytes.len());
        let (element_char_count, element_count, elements_unchanged) =
            super::response_telemetry::element_shape(&delivered_response);
        let response_send = send(socket, wire_response);
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
                "result_byte_count": result_byte_count,
                "response_byte_count": response_byte_count,
                "element_char_count": element_char_count,
                "element_count": element_count,
                "elements_unchanged": elements_unchanged,
                "generation_index": state.model_generation_index,
                "next_generation_index": state.model_generation_index.saturating_add(1),
                "observation_id": super::response_telemetry::nested_field(&delivered_response, "observation", "id"),
                "observation_status": super::response_telemetry::nested_field(&delivered_response, "observation", "status"),
                "error": response_send.as_ref().err().map(ToString::to_string),
            }),
        );
        response_send?;
        state.turn_tool_response_bytes = state
            .turn_tool_response_bytes
            .saturating_add(response_byte_count);
        state.turn_element_chars = state.turn_element_chars.saturating_add(element_char_count);
        state.model_generation_index = state.model_generation_index.saturating_add(1);
        record_observation(state, &name, &delivered_response);
        record_tool_result(state, &name);
        state
            .turn_outcomes
            .record_delivered(&name, &delivered_response);
        reconcile_from_observation(state, &name, response_ok);
        if let Some((b64, source)) = frame
            && super::frames::send_action_frame(socket, &b64, &source, action).is_ok()
        {
            state.source_frame = Some(source);
        }
        match response_transition(
            &name,
            response_ok,
            terminal_blocker,
            silent_terminal_blocker,
        ) {
            ResponseTransition::AcceptedDone => {
                close_accepted_done(state, sink, completion_generation_output_seen);
            }
            ResponseTransition::SilentTerminalBlocker => close_silent_terminal_blocker(state),
            ResponseTransition::TerminalBlocker => {
                close_terminal_blocker(state, sink, name == "done")
            }
            ResponseTransition::FailedDone => {
                super::speech_events::discard_failed_completion(
                    state,
                    sink,
                    "explicit_completion_failed",
                );
                state.awaiting = true;
                state.recovery_owed = true;
                state.think_start = Some(Instant::now());
            }
            ResponseTransition::Continue => {
                state.awaiting = true;
                state.recovery_owed = true;
                state.think_start = Some(Instant::now());
            }
        }
    }
    state.pending = Pending::default();
    state.pending_tool_output_seen = false;
    state.generation_output_seen = false;
    if !state.terminal_response.is_open() {
        state.pending_tool_boundary_seen = false;
    }
    state.nudged = false;
    overlay::set_status(result_status(state, reconnect_for_activation));
    Ok(Some(reconnect_for_activation))
}

fn close_accepted_done(state: &mut Reader, sink: Option<&AudioSink>, generation_output_seen: bool) {
    super::speech_events::release_generation_audio(state, sink, "explicit_completion_accepted");
    overlay::push_log("[done] model finished the turn".to_string());
    overlay::set_orb_done();
    emit_turn_summary(state, "done");
    if generation_output_seen {
        super::terminal_drain::finish_pre_tool_response(state);
    } else {
        super::terminal_drain::begin_final_response(state, true);
    }
}

pub(super) fn send_immediate_tool_responses(
    socket: &mut Sock,
    state: &mut Reader,
) -> anyhow::Result<bool> {
    if !immediate_tool_responses_ready(state) {
        return Ok(false);
    }
    let mut sent = false;
    let mut batch_bytes = 0usize;
    while let Some((id, name, response)) = state.immediate_tool_responses.pop_front() {
        let delivered_response = response.clone();
        let wire_response = tool_response(&id, &name, response);
        let response_bytes = serde_json::to_vec(&wire_response).map_or(0, |bytes| bytes.len());
        send(socket, wire_response)?;
        batch_bytes = batch_bytes.saturating_add(response_bytes);
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
                "response_byte_count": response_bytes,
                "generation_index": state.model_generation_index,
            }),
        );
        sent = true;
    }
    if sent {
        state.turn_tool_response_bytes = state.turn_tool_response_bytes.saturating_add(batch_bytes);
        state.model_generation_index = state.model_generation_index.saturating_add(1);
    }
    Ok(sent)
}

fn immediate_tool_responses_ready(state: &Reader) -> bool {
    state.pending.id.is_none() && !state.immediate_tool_responses.is_empty()
}

fn annotate_accepted_done_delivery(
    name: &str,
    response_ok: bool,
    completion_output_seen: bool,
    response: &mut Value,
) {
    if name != "done" || !response_ok || completion_output_seen || !response.is_object() {
        return;
    }
    response["final_response_required"] = Value::Bool(true);
    response["instruction"] = Value::String(
        "Speak the summary exactly once in one concise sentence. Do not call another tool or add another response."
            .to_string(),
    );
}

fn is_terminal_blocker(response: &Value) -> bool {
    response
        .get("terminal_blocker")
        .or_else(|| response.pointer("/action_result/terminal_blocker"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn is_silent_terminal_blocker(response: &Value) -> bool {
    response
        .get("silent_terminal_blocker")
        .or_else(|| response.pointer("/action_result/silent_terminal_blocker"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResponseTransition {
    AcceptedDone,
    SilentTerminalBlocker,
    TerminalBlocker,
    FailedDone,
    Continue,
}

fn response_transition(
    name: &str,
    response_ok: bool,
    terminal_blocker: bool,
    silent_terminal_blocker: bool,
) -> ResponseTransition {
    if name == "done" && response_ok {
        ResponseTransition::AcceptedDone
    } else if silent_terminal_blocker {
        ResponseTransition::SilentTerminalBlocker
    } else if terminal_blocker {
        ResponseTransition::TerminalBlocker
    } else if name == "done" {
        ResponseTransition::FailedDone
    } else {
        ResponseTransition::Continue
    }
}

fn close_silent_terminal_blocker(state: &mut Reader) {
    super::speech_events::discard_generation_audio(state, "silent_terminal_blocker");
    emit_turn_summary(state, "silent_terminal_blocker");
    super::reader_policy::begin_terminal_drain(state, false, false);
    overlay::set_orb_resting();
}

fn close_terminal_blocker(
    state: &mut Reader,
    sink: Option<&AudioSink>,
    discard_failed_completion: bool,
) {
    if discard_failed_completion {
        super::speech_events::discard_failed_completion(state, sink, "terminal_completion_blocked");
    }
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
    use super::super::effect_receipt::EffectStatus;

    match EffectStatus::from_value(response) {
        EffectStatus::Verified => ("effect_completed_response_not_delivered", true),
        EffectStatus::MayHaveOccurred => ("effect_may_have_occurred_response_not_delivered", true),
        EffectStatus::ProvenNoEffect => ("proven_no_effect_response_not_delivered", false),
        EffectStatus::Unknown => ("effect_unknown_response_not_delivered", true),
    }
}

fn settle_transport_receipt(state: &mut Reader, tool: &str, response: &Value) {
    use super::super::effect_receipt::EffectStatus;

    if !super::super::turn_policy::is_mutating_tool(tool) {
        return;
    }
    let effect_status = EffectStatus::from_value(response);
    if effect_status.is_proven_no_effect() {
        state.reconciliation_required = false;
        state.turn_outcomes.clear_interruption_uncertainty();
        telemetry::event(
            "interrupted_effect_reconciled",
            "runtime",
            Privacy::Safe,
            serde_json::json!({
                "tool": tool,
                "basis": "worker_receipt_proven_no_effect",
            }),
        );
    } else {
        let newly_required = !state.reconciliation_required;
        state.reconciliation_required = true;
        if newly_required {
            state.turn_outcomes.record_interrupted_effect(tool);
        }
        telemetry::event(
            "interrupted_effect_reconciliation_required",
            "runtime",
            Privacy::Safe,
            serde_json::json!({
                "tool": tool,
                "effect_status": effect_status.code(),
                "basis": "worker_receipt_not_delivered",
            }),
        );
    }
}

fn reconcile_from_observation(state: &mut Reader, tool: &str, response_ok: bool) {
    if !state.reconciliation_required
        || !response_ok
        || !super::super::turn_policy::provides_reconciliation_evidence(tool)
    {
        return;
    }
    state.reconciliation_required = false;
    state.turn_outcomes.clear_interruption_uncertainty();
    telemetry::event(
        "interrupted_effect_reconciled",
        "runtime",
        Privacy::Safe,
        serde_json::json!({
            "tool": tool,
            "basis": "fresh_capability_observation",
        }),
    );
}

#[cfg(test)]
#[path = "results_claim_tests.rs"]
mod claim_tests;
#[cfg(test)]
#[path = "results_tests.rs"]
mod tests;
