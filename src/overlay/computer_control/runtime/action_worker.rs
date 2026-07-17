//! Runs model tool calls on the shared `Brain`; communicates through `Job` / `Done`.
use super::super::effect_receipt::EffectStatus;
use super::super::telemetry::{self, Privacy};
use super::super::turn_policy;
use super::super::uia_task::Brain;
use super::action_worker_receive::receive_until_stopped;
use super::completion_responses::accepted_done_response;
use super::effect_reporting::{
    cancelled, cancelled_after_dispatch, complete_observation_after_dispatch,
    complete_structured_result_after_dispatch, proven_no_effect_after_dispatch,
    unavailable_postcondition,
};
use super::repeat_failure::RepeatFailureGuard;
use super::{Done, Job};
use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
/// Execute queued calls on the shared `Brain` and return grounded results.
/// Cancellation is job-scoped, so later work cannot revive a cancelled call.
pub(super) fn executor_loop(
    target: Option<String>,
    rx: mpsc::Receiver<Job>,
    tx: mpsc::Sender<Done>,
    cleanup_tx: mpsc::Sender<u64>,
    stop: Arc<AtomicBool>,
) {
    let mut brain = Brain::new(target);
    let mut repeat_failures = RepeatFailureGuard::default();
    while let Some(job) = receive_until_stopped(&rx, &stop) {
        let Job {
            id,
            name,
            args,
            task,
            user_text,
            inherit_evidence,
            action,
            source_frame,
            queued_at,
            cancel,
        } = job;
        if name == super::RETIRE_TURN {
            brain.retire_turn(action.turn_id);
            telemetry::event(
                "turn_cleanup_completed",
                "runtime",
                Privacy::Safe,
                json!({
                    "cleanup_turn_id": action.turn_id,
                    "source": "model_turn_complete",
                }),
            );
            let _ = cleanup_tx.send(action.turn_id);
            continue;
        }
        brain.record_user_request(action.turn_id, &user_text);
        brain.begin_job(action.turn_id, source_frame.clone(), inherit_evidence);
        let queue_ms = queued_at.elapsed().as_millis();
        let action_started = std::time::Instant::now();
        let (response, frame) = if cancel.load(Ordering::SeqCst) {
            brain.interrupt_turn(action.turn_id);
            cancelled("before_dispatch", EffectStatus::ProvenNoEffect)
        } else if name == "done" {
            let summary = args
                .get("summary")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            brain.retire_turn(action.turn_id);
            telemetry::event_for_action(
                "turn_cleanup_completed",
                "runtime",
                Privacy::Safe,
                action,
                json!({
                    "cleanup_turn_id": action.turn_id,
                    "source": "explicit_done",
                }),
            );
            telemetry::event_for_action(
                "completion_declared",
                "runtime",
                Privacy::Safe,
                action,
                json!({"summary_char_count": summary.chars().count()}),
            );
            (accepted_done_response(summary), None)
        } else {
            let dispatch_name = name.clone();
            let dispatch_args = args.clone();
            let ctx = format!("user request: {user_text}");
            let dispatch_started = std::time::Instant::now();
            let failure_surface = source_frame.as_ref().map(|source| &source.surface);
            let blocked = repeat_failures.blocked_result(
                action.turn_id,
                &dispatch_name,
                &dispatch_args,
                failure_surface,
            );
            let outcome_guarded = blocked.is_some();
            let action_result = if let Some(blocked) = blocked {
                telemetry::event_for_action(
                    "action_guard_blocked",
                    "action_worker",
                    Privacy::Safe,
                    action,
                    json!({
                        "tool": dispatch_name.clone(),
                        "guard": "repeat_outcome",
                        "code": blocked.get("code"),
                        "effect_may_have_occurred": false,
                    }),
                );
                blocked
            } else {
                brain.dispatch(
                    &dispatch_name,
                    &dispatch_args,
                    &ctx,
                    &cancel,
                    Some(action),
                    false,
                )
            };
            if !outcome_guarded
                && repeat_failures.observe(
                    action.turn_id,
                    &dispatch_name,
                    &dispatch_args,
                    failure_surface,
                    &action_result,
                )
            {
                telemetry::event_for_action(
                    "repeat_outcome_threshold_reached",
                    "action_worker",
                    Privacy::Safe,
                    action,
                    json!({
                        "tool": dispatch_name.clone(),
                    }),
                );
            }
            let dispatch_ms = dispatch_started.elapsed().as_millis();
            if cancel.load(Ordering::SeqCst) {
                brain.interrupt_turn(action.turn_id);
                cancelled_after_dispatch(
                    action_result,
                    turn_policy::is_mutating_tool(&dispatch_name),
                )
            } else {
                let mutating = turn_policy::is_mutating_tool(&dispatch_name);
                let dispatch_effect = EffectStatus::after_dispatch(&action_result, mutating);
                if dispatch_effect.is_proven_no_effect() {
                    telemetry::event_for_action(
                        "postcondition",
                        "action_worker",
                        Privacy::Safe,
                        action,
                        json!({
                            "tool": dispatch_name.clone(),
                            "status": "not_run",
                            "reason": "dispatch_proved_no_effect",
                        }),
                    );
                    proven_no_effect_after_dispatch(action_result, queue_ms, dispatch_ms)
                } else if dispatch_effect.is_verified()
                    || turn_policy::has_nonvisual_structured_receipt(&dispatch_name, &action_result)
                {
                    complete_structured_result_after_dispatch(
                        action_result,
                        queue_ms,
                        dispatch_ms,
                        dispatch_effect,
                    )
                } else if !turn_policy::requires_post_dispatch_grounding(&dispatch_name) {
                    telemetry::event_for_action(
                        "postcondition",
                        "action_worker",
                        Privacy::Safe,
                        action,
                        json!({"tool": dispatch_name.clone(), "status": "not_applicable"}),
                    );
                    complete_observation_after_dispatch(action_result, queue_ms, dispatch_ms)
                } else {
                    let ground_started = std::time::Instant::now();
                    match brain.ground(&dispatch_name, &dispatch_args) {
                        Ok(g) => {
                            let execution_ok = action_result.get("ok").and_then(|v| v.as_bool());
                            let effect_status =
                                EffectStatus::after_dispatch(&action_result, mutating);
                            let mut resp = json!({
                                "action_result": action_result,
                                "execution_ok": execution_ok,
                                "new_state": g.state_text,
                                "timing": {
                                    "queue_ms": queue_ms,
                                    "dispatch_and_settle_ms": dispatch_ms,
                                    "ground_ms": ground_started.elapsed().as_millis(),
                                },
                            });
                            if let Some(ok) = execution_ok {
                                resp["ok"] = json!(ok);
                            }
                            let recovery_advice = (!cancel.load(Ordering::SeqCst)
                                && execution_ok != Some(false)
                                && g.postcondition.request_advice())
                            .then(|| brain.stuck_advice(&task, &cancel))
                            .flatten();
                            resp["postcondition"] = g.postcondition.response(
                                execution_ok,
                                mutating,
                                effect_status,
                                recovery_advice,
                            );
                            if execution_ok == Some(false) {
                                resp["ok"] = json!(false);
                            } else if g.postcondition.detected_no_effect()
                                && !effect_status.is_verified()
                            {
                                resp["ok"] = json!(false);
                                telemetry::typed_error(
                                    "ERR_POSTCONDITION_NO_EFFECT",
                                    "action_worker",
                                    "grounding did not confirm a useful effect; receipt certainty is preserved separately",
                                    json!({
                                        "tool": dispatch_name.clone(),
                                        "effect_status": effect_status.code(),
                                        "repeated": g.postcondition.repeated(),
                                    }),
                                );
                            } else if effect_status.is_verified() {
                                telemetry::event_for_action(
                                    "postcondition",
                                    "action_worker",
                                    Privacy::Safe,
                                    action,
                                    json!({
                                        "tool": dispatch_name.clone(),
                                        "status": "confirmed",
                                        "confirmed": true,
                                    }),
                                );
                            } else if !mutating {
                                telemetry::event_for_action(
                                    "postcondition",
                                    "action_worker",
                                    Privacy::Safe,
                                    action,
                                    json!({"tool": dispatch_name.clone(), "status": "not_applicable"}),
                                );
                            } else {
                                let status = if effect_status.is_maybe() {
                                    "inconclusive"
                                } else {
                                    "not_disproven"
                                };
                                telemetry::event_for_action(
                                    "postcondition",
                                    "action_worker",
                                    Privacy::Safe,
                                    action,
                                    json!({
                                        "tool": dispatch_name.clone(),
                                        "status": status,
                                        "confirmed": false,
                                        "effect_status": effect_status.code(),
                                    }),
                                );
                            }
                            effect_status.annotate(&mut resp);
                            (resp, Some((g.frame_b64, g.source)))
                        }
                        Err(e) => {
                            let execution_ok = action_result.get("ok").and_then(|v| v.as_bool());
                            let effect_status =
                                EffectStatus::after_dispatch(&action_result, mutating);
                            telemetry::typed_error(
                                "ERR_POSTCONDITION_UNAVAILABLE",
                                "action_worker",
                                "could not ground the desktop after tool execution",
                                json!({
                                    "tool": dispatch_name.clone(),
                                    "error": telemetry::value_metadata(&json!(e.to_string())),
                                }),
                            );
                            let mut response = json!({
                                "ok": execution_ok.unwrap_or(false),
                                "execution_ok": execution_ok,
                                "action_result": action_result,
                                "ground_error": e.to_string(),
                                "timing": {
                                    "queue_ms": queue_ms,
                                    "dispatch_and_settle_ms": dispatch_ms,
                                    "ground_ms": ground_started.elapsed().as_millis(),
                                },
                                "postcondition": unavailable_postcondition(effect_status),
                            });
                            effect_status.annotate(&mut response);
                            (response, None)
                        }
                    }
                }
            }
        };
        record_action_failure(action, &name, &response);
        let effect_status = EffectStatus::from_value(&response);
        if effect_status.is_verified() {
            repeat_failures.clear_after_verified_progress(action.turn_id);
        }
        telemetry::event_for_action(
            "action_outcome",
            "action_worker",
            Privacy::Safe,
            action,
            json!({
                "tool_call_id": id.clone(),
                "requested_tool": name.clone(),
                "effective_tool": name.clone(),
                "executed": effect_status.executed(),
                "effect_status": effect_status.code(),
                "effect_verified": effect_status.is_verified(),
                "effect_may_have_occurred": effect_status.may_have_occurred(),
                "ok": response.get("ok"),
                "execution_ok": response.get("execution_ok"),
                "cancelled": response.get("cancelled"),
                "cancel_stage": response.get("stage"),
                "source_frame_id": source_frame.as_ref().map(|frame| frame.frame_id),
                "source_surface": source_frame.as_ref().map(|frame| &frame.surface),
                "post_frame_id": frame.as_ref().map(|(_, source)| source.frame_id),
                "post_surface": frame.as_ref().map(|(_, source)| &source.surface),
                "queue_ms": queue_ms,
                "total_worker_ms": action_started.elapsed().as_millis(),
                "dispatch_ms": response.pointer("/timing/dispatch_and_settle_ms")
                    .or_else(|| response.pointer("/timing/dispatch_ms")),
                "ground_ms": response.pointer("/timing/ground_ms"),
                "grounding_performed": response.pointer("/grounding/performed"),
                "tab_lifetime": receipt_field(&response, "lifetime"),
                "postcondition": response.get("postcondition"),
                "error_code": receipt_field(&response, "code"),
                "failure_reason": receipt_field(&response, "reason"),
                "failure_phase": receipt_field(&response, "phase"),
                "injection": super::super::executor::input_injection(&response),
            }),
        );
        let done: Done = (id, name, response, frame, cancel, action);
        if tx.send(done).is_err() {
            break;
        }
    }
}

fn receipt_field<'a>(
    response: &'a serde_json::Value,
    field: &str,
) -> Option<&'a serde_json::Value> {
    response.get(field).or_else(|| {
        response
            .get("action_result")
            .and_then(|value| value.get(field))
    })
}

fn record_action_failure(action: telemetry::ActionTrace, tool: &str, result: &serde_json::Value) {
    if result.get("ok").and_then(serde_json::Value::as_bool) != Some(false) {
        return;
    }
    let code = action_failure_code(result);
    telemetry::typed_error_for_action(
        code,
        "action_worker",
        "tool dispatch reported a structured failure",
        action,
        json!({
            "tool": tool,
            "reason": receipt_field(result, "reason"),
            "phase": receipt_field(result, "phase"),
            "dispatch_ok": receipt_field(result, "dispatch_ok"),
            "effect_may_have_occurred": result.get("effect_may_have_occurred"),
            "observation_status": result.pointer("/observation/status")
                .or_else(|| result.pointer("/action_result/observation/status")),
            "observation_count": result.pointer("/observation/count")
                .or_else(|| result.pointer("/action_result/observation/count")),
        }),
    );
}

fn action_failure_code(result: &serde_json::Value) -> &str {
    receipt_field(result, "code")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("ERR_ACTION_EXECUTION_FAILED")
}
#[cfg(test)]
#[path = "action_worker_tests.rs"]
mod tests;
