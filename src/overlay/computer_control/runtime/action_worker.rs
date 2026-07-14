//! The action-worker thread: owns the shared `Brain` and runs each tool call the
//! Live model emits to completion (humanized, cancellable), so a slow action can
//! glide while the session/reader thread keeps receiving mic + barge-in. It talks
//! to the session loop only through the `Job` / `Done` channels.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{Value, json};

use super::super::telemetry::{self, Privacy};
use super::super::turn_policy;
use super::super::uia_task::Brain;
use super::repeat_failure::RepeatFailureGuard;
use super::{Done, Job};

/// Drain `rx` of tool calls, executing each on the shared `Brain` and returning
/// the grounded result (+ next frame) on `tx`. Cancellation is scoped to each
/// queued job, so a later action cannot clear an earlier stop request.
pub(super) fn executor_loop(
    target: Option<String>,
    rx: mpsc::Receiver<Job>,
    tx: mpsc::Sender<Done>,
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
            action,
            source_frame,
            queued_at,
            cancel,
        } = job;
        brain.begin_job(action.turn_id, source_frame.clone());
        let queue_ms = queued_at.elapsed().as_millis();
        let action_started = std::time::Instant::now();
        let (response, frame) = if cancel.load(Ordering::SeqCst) {
            brain.retire_turn(action.turn_id);
            cancelled("before_dispatch")
        } else if name == "done" {
            // Completion evidence comes from an independent high-resolution check.
            let t0 = std::time::Instant::now();
            let verifier_goal = turn_policy::verification_goal(&user_text, &args);
            let check = brain.verify_done(&verifier_goal, &cancel);
            let ok = check.complete;
            let verifier_unavailable = check.unavailable;
            let verdict = check.verdict;
            let duration_ms = t0.elapsed().as_millis();
            telemetry::human(
                "cc",
                format!(
                    "DONE-claim verifier complete={ok} ({} chars)",
                    verdict.chars().count()
                ),
            );
            telemetry::event_for_action(
                "done_verifier_result",
                "done_verifier",
                Privacy::Safe,
                action,
                json!({
                    "ok": ok,
                    "duration_ms": duration_ms,
                    "verdict_char_count": verdict.chars().count(),
                    "goal_source": "committed_user_transcript",
                    "user_goal_char_count": user_text.chars().count(),
                    "done_claim_char_count": args.get("summary").and_then(|v| v.as_str()).unwrap_or("").chars().count(),
                    "verifier_goal_char_count": verifier_goal.chars().count(),
                }),
            );
            if cancel.load(Ordering::SeqCst) {
                brain.retire_turn(action.turn_id);
                cancelled("during_done_verification")
            } else if ok {
                brain.retire_turn(action.turn_id);
                (json!({"ok": true, "verdict": verdict}), None)
            } else if verifier_unavailable {
                brain.retire_turn(action.turn_id);
                telemetry::typed_error(
                    "ERR_DONE_VERIFIER_UNAVAILABLE",
                    "done_verifier",
                    "independent completion verification was unavailable",
                    json!({"verdict_char_count": verdict.chars().count()}),
                );
                (
                    json!({
                        "ok": false,
                        "code": "ERR_DONE_VERIFIER_UNAVAILABLE",
                        "terminal_blocker": true,
                        "verdict": verdict,
                        "instruction": "Do not continue acting or retry completion. Report this verification blocker once, then end the turn."
                    }),
                    None,
                )
            } else {
                brain.bind_action(action);
                let (state_text, frame) = match brain.ground(&name, &args) {
                    Ok(g) => (g.state_text, Some((g.frame_b64, g.source))),
                    Err(e) => (format!("(ground failed: {e})"), None),
                };
                (
                    json!({
                        "ok": false,
                        "independent_check": verdict,
                        "instruction": "An independent high-res check says the goal is NOT yet achieved. Keep \
                    working until it is actually done.",
                        "new_state": state_text,
                    }),
                    frame,
                )
            }
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
            let failure_guarded = blocked.is_some();
            let action_result = if let Some(blocked) = blocked {
                telemetry::event_for_action(
                    "repeat_failure_guard_blocked",
                    "action_worker",
                    Privacy::Safe,
                    action,
                    json!({
                        "tool": dispatch_name.clone(),
                        "failure_limit": 2,
                        "effect_may_have_occurred": false,
                    }),
                );
                blocked
            } else {
                brain.dispatch(&dispatch_name, &dispatch_args, &ctx, &cancel, Some(action))
            };
            if !failure_guarded
                && repeat_failures.observe(
                    action.turn_id,
                    &dispatch_name,
                    &dispatch_args,
                    failure_surface,
                    &action_result,
                )
            {
                telemetry::event_for_action(
                    "repeat_failure_threshold_reached",
                    "action_worker",
                    Privacy::Safe,
                    action,
                    json!({
                        "tool": dispatch_name.clone(),
                        "failure_limit": 2,
                    }),
                );
            }
            let dispatch_ms = dispatch_started.elapsed().as_millis();
            if cancel.load(Ordering::SeqCst) {
                brain.retire_turn(action.turn_id);
                let (mut response, frame) = cancelled("after_dispatch");
                response["action_result"] = action_result;
                (response, frame)
            } else {
                let ground_started = std::time::Instant::now();
                match brain.ground(&dispatch_name, &dispatch_args) {
                    Ok(g) => {
                        let execution_ok = action_result.get("ok").and_then(|v| v.as_bool());
                        let effect_verified = action_result
                            .get("effect_verified")
                            .and_then(|value| value.as_bool())
                            == Some(true);
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
                            turn_policy::is_mutating_tool(&dispatch_name),
                            effect_verified,
                            recovery_advice,
                        );
                        if execution_ok == Some(false) {
                            resp["ok"] = json!(false);
                            telemetry::typed_error(
                                "ERR_ACTION_EXECUTION_FAILED",
                                "action_worker",
                                "tool execution reported failure; no successful effect was claimed",
                                json!({
                                    "tool": dispatch_name.clone(),
                                    "action_result": telemetry::value_metadata(&resp["action_result"]),
                                }),
                            );
                        } else if g.postcondition.detected_no_effect() && !effect_verified {
                            resp["ok"] = json!(false);
                            telemetry::typed_error(
                                "ERR_POSTCONDITION_NO_EFFECT",
                                "action_worker",
                                "grounding detected no useful effect after an action",
                                json!({
                                    "tool": dispatch_name.clone(),
                                    "repeated": g.postcondition.repeated(),
                                }),
                            );
                        } else if effect_verified {
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
                        } else if !turn_policy::is_mutating_tool(&dispatch_name) {
                            telemetry::event_for_action(
                                "postcondition",
                                "action_worker",
                                Privacy::Safe,
                                action,
                                json!({"tool": dispatch_name.clone(), "status": "not_applicable"}),
                            );
                        } else {
                            telemetry::event_for_action(
                                "postcondition",
                                "action_worker",
                                Privacy::Safe,
                                action,
                                json!({
                                    "tool": dispatch_name.clone(),
                                    "status": "not_disproven",
                                    "confirmed": false,
                                }),
                            );
                        }
                        (resp, Some((g.frame_b64, g.source)))
                    }
                    Err(e) => {
                        let execution_ok = action_result.get("ok").and_then(|v| v.as_bool());
                        telemetry::typed_error(
                            "ERR_POSTCONDITION_UNAVAILABLE",
                            "action_worker",
                            "could not ground the desktop after tool execution",
                            json!({
                                "tool": dispatch_name.clone(),
                                "error": telemetry::value_metadata(&json!(e.to_string())),
                            }),
                        );
                        (
                            json!({
                                "ok": execution_ok.unwrap_or(false),
                                "execution_ok": execution_ok,
                                "action_result": action_result,
                            "ground_error": e.to_string(),
                            "timing": {
                                "queue_ms": queue_ms,
                                "dispatch_and_settle_ms": dispatch_ms,
                                "ground_ms": ground_started.elapsed().as_millis(),
                            },
                                "postcondition": {
                                    "ok": null,
                                    "status": "unavailable",
                                    "confirmed": false,
                                    "effect": "unknown",
                                },
                            }),
                            None,
                        )
                    }
                }
            }
        };
        telemetry::event_for_action(
            "action_outcome",
            "action_worker",
            Privacy::Safe,
            action,
            json!({
                "tool_call_id": id.clone(),
                "requested_tool": name.clone(),
                "effective_tool": name.clone(),
                "executed": response
                    .get("executed")
                    .or_else(|| response.pointer("/action_result/executed"))
                    .and_then(Value::as_bool)
                    .unwrap_or_else(|| !response.get("cancelled").and_then(Value::as_bool).unwrap_or(false)),
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
                "postcondition": response.get("postcondition"),
                "injection": super::super::executor::input_injection(&response),
            }),
        );
        let done: Done = (id, name, response, frame, cancel, action);
        if tx.send(done).is_err() {
            break;
        }
    }
}

const RECEIVE_POLL_INTERVAL: Duration = Duration::from_millis(50);

/// A session stop must wake an idle executor even while another sender clone
/// still exists. Channel disconnection remains a second, independent exit path.
fn receive_until_stopped<T>(rx: &mpsc::Receiver<T>, stop: &AtomicBool) -> Option<T> {
    loop {
        if stop.load(Ordering::SeqCst) {
            return None;
        }
        match rx.recv_timeout(RECEIVE_POLL_INTERVAL) {
            Ok(value) => return Some(value),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return None,
        }
    }
}

fn cancelled(
    stage: &str,
) -> (
    serde_json::Value,
    Option<(String, super::super::uia_task::FrameSource)>,
) {
    (
        json!({
            "ok": false,
            "status": "aborted_by_user",
            "cancelled": true,
            "stage": stage,
            "postcondition": {
                "ok": false,
                "status": "not_run",
                "effect": "unknown",
                "reason": "cancelled",
            },
        }),
        None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_receiver_stops_even_when_a_sender_is_still_alive() {
        let (tx, rx) = mpsc::channel::<()>();
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let worker = std::thread::spawn(move || receive_until_stopped(&rx, &worker_stop));

        stop.store(true, Ordering::SeqCst);
        assert_eq!(worker.join().unwrap(), None);
        drop(tx);
    }

    #[test]
    fn disconnected_receiver_stops_without_a_session_signal() {
        let (tx, rx) = mpsc::channel::<()>();
        let stop = AtomicBool::new(false);
        drop(tx);

        assert_eq!(receive_until_stopped(&rx, &stop), None);
    }
}
