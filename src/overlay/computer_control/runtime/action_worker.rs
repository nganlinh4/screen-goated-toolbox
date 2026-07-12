//! The action-worker thread: owns the shared `Brain` and runs each tool call the
//! Live model emits to completion (humanized, cancellable), so a slow action can
//! glide while the session/reader thread keeps receiving mic + barge-in. It talks
//! to the session loop only through the `Job` / `Done` channels.

use std::sync::atomic::Ordering;
use std::sync::mpsc;

use serde_json::json;

use super::super::telemetry::{self, Privacy};
use super::super::turn_policy;
use super::super::uia_task::Brain;
use super::{Done, Job};

/// Drain `rx` of tool calls, executing each on the shared `Brain` and returning
/// the grounded result (+ next frame) on `tx`. Cancellation is scoped to each
/// queued job, so a later action cannot clear an earlier stop request.
pub(super) fn executor_loop(
    target: Option<String>,
    rx: mpsc::Receiver<Job>,
    tx: mpsc::Sender<Done>,
) {
    let mut brain = Brain::new(target);
    let mut research_failed_turn = None;
    while let Ok(job) = rx.recv() {
        let Job {
            id,
            name,
            args,
            task,
            intent,
            user_text,
            action,
            source_frame_id,
            queued_at,
            cancel,
        } = job;
        let queue_ms = queued_at.elapsed().as_millis();
        let action_started = std::time::Instant::now();
        let (response, frame) = if cancel.load(Ordering::SeqCst) {
            cancelled("before_dispatch")
        } else if name == "done" {
            // Independent high-res check - the Live agent confabulates success.
            let t0 = std::time::Instant::now();
            let verifier_goal = turn_policy::verification_goal(&user_text, &task, &args);
            let (ok, verdict) = brain.verify_done(&verifier_goal, &cancel);
            let duration_ms = t0.elapsed().as_millis();
            telemetry::human("cc", format!("DONE-claim verdict: {verdict}"));
            telemetry::event_for_action(
                "done_verifier_result",
                "done_verifier",
                Privacy::Safe,
                action,
                json!({
                    "ok": ok,
                    "duration_ms": duration_ms,
                    "verdict_preview": verdict.chars().take(500).collect::<String>(),
                    "goal_source": "user_transcript_with_secondary_model_context",
                    "user_goal_preview": user_text.chars().take(240).collect::<String>(),
                    "model_intent_preview": task.chars().take(240).collect::<String>(),
                    "done_claim_preview": args.get("summary").and_then(|v| v.as_str()).unwrap_or("").chars().take(240).collect::<String>(),
                    "verifier_goal_preview": verifier_goal.chars().take(720).collect::<String>(),
                }),
            );
            if cancel.load(Ordering::SeqCst) {
                cancelled("during_done_verification")
            } else if ok {
                (json!({"ok": true, "verdict": verdict}), None)
            } else {
                brain.bind_action(action);
                let (state_text, frame) = match brain.ground(&name, &args) {
                    Ok(g) => (g.state_text, Some((g.frame_b64, g.frame_id))),
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
            let research_circuit_open = research_failed_turn == Some(action.turn_id);
            let route_args = (!research_circuit_open)
                .then(|| turn_policy::auto_research_args(&user_text, &task, &intent, &name, &args))
                .flatten();
            let (dispatch_name, dispatch_args, rerouted_from) = match route_args {
                Some(research_args) => {
                    telemetry::human(
                        "cc",
                        format!("policy reroute requested={name} effective=research_web"),
                    );
                    telemetry::typed_error(
                        "ERR_WEAK_TOOL_FOR_TURN",
                        "turn_policy",
                        "rerouted weak tool to research_web",
                        json!({
                            "requested_tool": name.clone(),
                            "effective_tool": "research_web",
                            "task": task.chars().take(240).collect::<String>(),
                            "intent": intent.chars().take(240).collect::<String>(),
                        }),
                    );
                    telemetry::event_for_action(
                        "policy_reroute",
                        "turn_policy",
                        Privacy::Safe,
                        action,
                        json!({
                            "tool_call_id": id.clone(),
                            "requested_tool": name.clone(),
                            "effective_tool": "research_web",
                        }),
                    );
                    (
                        "research_web".to_string(),
                        research_args,
                        Some(name.clone()),
                    )
                }
                None => (name.clone(), args.clone(), None),
            };
            let ctx = format!("user request: {user_text}");
            let dispatch_started = std::time::Instant::now();
            let action_result = if research_circuit_open && dispatch_name == "research_web" {
                json!({
                    "ok": false,
                    "code": "ERR_RESEARCH_CIRCUIT_OPEN",
                    "error": "research_web already failed during this user turn",
                    "instruction": "Do not retry research_web this turn. Use a different available read path once, or explain the limitation once and finish.",
                })
            } else {
                brain.dispatch(&dispatch_name, &dispatch_args, &ctx, &cancel, Some(action))
            };
            if dispatch_name == "research_web"
                && action_result.get("ok").and_then(|value| value.as_bool()) == Some(false)
            {
                research_failed_turn = Some(action.turn_id);
                telemetry::event_for_action(
                    "research_circuit_opened",
                    "research",
                    Privacy::Safe,
                    action,
                    json!({"code": action_result.get("code")}),
                );
            }
            let dispatch_ms = dispatch_started.elapsed().as_millis();
            if cancel.load(Ordering::SeqCst) {
                let (mut response, frame) = cancelled("after_dispatch");
                response["action_result"] = action_result;
                (response, frame)
            } else {
                let ground_started = std::time::Instant::now();
                match brain.ground(&dispatch_name, &dispatch_args) {
                    Ok(g) => {
                        let execution_ok = action_result.get("ok").and_then(|v| v.as_bool());
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
                        if turn_policy::request_is_edit_only(&user_text)
                            && action_result.get("did").and_then(|value| value.as_str())
                                == Some("fill")
                            && execution_ok == Some(true)
                        {
                            resp["scope_completion"] = json!({
                                "requested_effect_complete": true,
                                "submission_forbidden": true,
                                "instruction": "The requested edit is verified. Call done now; do not click, submit, send, or continue the prior task."
                            });
                        }
                        if let Some(from) = rerouted_from {
                            resp["policy"] = json!({
                                "rerouted_from": from,
                                "actual_tool": dispatch_name,
                                "reason": "weak tool for current user turn",
                            });
                        }
                        for (k, v) in &g.notes {
                            resp[*k] = json!(*v);
                        }
                        let no_effect = g.notes.iter().any(|(k, _)| {
                            matches!(
                                *k,
                                "screen_change"
                                    | "ui_change"
                                    | "stuck_warning"
                                    | "postcondition_block"
                            )
                        });
                        if execution_ok == Some(false) {
                            resp["ok"] = json!(false);
                            resp["postcondition"] = json!({
                                "ok": false,
                                "status": "not_run",
                                "effect": "unknown",
                                "reason": "execution_failed",
                            });
                            telemetry::typed_error(
                                "ERR_ACTION_EXECUTION_FAILED",
                                "action_worker",
                                "tool execution reported failure; no successful effect was claimed",
                                json!({"tool": dispatch_name.clone(), "action_result": resp["action_result"]}),
                            );
                        } else if no_effect {
                            let repeated = g.notes.iter().any(|(k, _)| *k == "postcondition_block");
                            resp["ok"] = json!(false);
                            resp["postcondition"] = json!({
                                "ok": false,
                                "status": "checked",
                                "effect": "none_detected",
                                "repeated": repeated,
                                "instruction": if repeated {
                                    "Do not retry the same action. Use a different route or stop with the blocker."
                                } else {
                                    "Re-observe/replan or change tool family before acting again."
                                },
                            });
                            telemetry::typed_error(
                                "ERR_POSTCONDITION_NO_EFFECT",
                                "action_worker",
                                "grounding detected no useful effect after an action",
                                json!({
                                    "tool": dispatch_name.clone(),
                                    "repeated": repeated,
                                    "notes": g.notes.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
                                }),
                            );
                        } else if !turn_policy::is_mutating_tool(&dispatch_name) {
                            resp["postcondition"] = json!({
                                "status": "not_applicable",
                                "effect": "observation_or_query",
                            });
                            telemetry::event_for_action(
                                "postcondition",
                                "action_worker",
                                Privacy::Safe,
                                action,
                                json!({"tool": dispatch_name.clone(), "status": "not_applicable"}),
                            );
                        } else {
                            resp["postcondition"] = json!({
                                "ok": null,
                                "status": "not_disproven",
                                "confirmed": false,
                                "effect": "no_failure_observed",
                            });
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
                        // On a detected stall, spend ONE grounded vision call (the merge
                        // planner) to propose a concrete next action — perception + plan in
                        // one shot, replacing the generic stuck warning with what it SEES.
                        if !cancel.load(Ordering::SeqCst)
                            && g.notes.iter().any(|(k, _)| *k == "stuck_warning")
                            && let Some(advice) = brain.stuck_advice(&task, &cancel)
                        {
                            resp["stuck_advice"] = json!(advice);
                        }
                        (resp, Some((g.frame_b64, g.frame_id)))
                    }
                    Err(e) => {
                        let execution_ok = action_result.get("ok").and_then(|v| v.as_bool());
                        telemetry::typed_error(
                            "ERR_POSTCONDITION_UNAVAILABLE",
                            "action_worker",
                            "could not ground the desktop after tool execution",
                            json!({"tool": dispatch_name.clone(), "error": e.to_string()}),
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
        let effective_tool = response
            .pointer("/policy/actual_tool")
            .and_then(|value| value.as_str())
            .unwrap_or(&name);
        telemetry::event_for_action(
            "action_outcome",
            "action_worker",
            Privacy::Safe,
            action,
            json!({
                "tool_call_id": id.clone(),
                "requested_tool": name.clone(),
                "effective_tool": effective_tool,
                "executed": !response.get("cancelled").and_then(|value| value.as_bool()).unwrap_or(false),
                "ok": response.get("ok"),
                "execution_ok": response.get("execution_ok"),
                "cancelled": response.get("cancelled"),
                "cancel_stage": response.get("stage"),
                "source_frame_id": source_frame_id,
                "post_frame_id": frame.as_ref().map(|(_, frame_id)| frame_id),
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

fn cancelled(stage: &str) -> (serde_json::Value, Option<(String, u64)>) {
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
