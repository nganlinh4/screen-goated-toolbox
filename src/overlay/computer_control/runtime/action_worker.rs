//! The action-worker thread: owns the shared `Brain` and runs each tool call the
//! Live model emits to completion (humanized, cancellable), so a slow action can
//! glide while the session/reader thread keeps receiving mic + barge-in. It talks
//! to the session loop only through the `Job` / `Done` channels.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

use serde_json::json;

use super::super::telemetry::{self, Privacy};
use super::super::turn_policy;
use super::super::uia_task::Brain;
use super::{Done, Job};

/// Drain `rx` of tool calls, executing each on the shared `Brain` and returning
/// the grounded result (+ next frame) on `tx`. `cancel` is reset per action and
/// flipped on barge-in to halt SendInput mid-glide.
pub(super) fn executor_loop(
    target: Option<String>,
    rx: mpsc::Receiver<Job>,
    tx: mpsc::Sender<Done>,
    cancel: Arc<AtomicBool>,
) {
    let mut brain = Brain::new(target);
    while let Ok((id, name, args, task, intent, user_text)) = rx.recv() {
        cancel.store(false, Ordering::SeqCst); // each action starts fresh
        let done: Done = if name == "done" {
            // Independent high-res check - the Live agent confabulates success.
            let t0 = std::time::Instant::now();
            let (ok, verdict) = brain.verify_done(&task, &cancel);
            let duration_ms = t0.elapsed().as_millis();
            telemetry::human("cc", format!("DONE-claim verdict: {verdict}"));
            telemetry::event(
                "done_verifier_result",
                "done_verifier",
                Privacy::Safe,
                json!({
                    "ok": ok,
                    "duration_ms": duration_ms,
                    "verdict_preview": verdict.chars().take(500).collect::<String>(),
                    "task_preview": task.chars().take(240).collect::<String>(),
                }),
            );
            if ok {
                (id, name, json!({"ok": true, "verdict": verdict}), None)
            } else {
                let (state_text, frame) = match brain.ground(&name, &args) {
                    Ok(g) => (g.state_text, Some(g.frame_b64)),
                    Err(e) => (format!("(ground failed: {e})"), None),
                };
                (
                    id,
                    name,
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
            let route_args =
                turn_policy::auto_research_args(&user_text, &task, &intent, &name, &args);
            let (dispatch_name, dispatch_args, rerouted_from) = match route_args {
                Some(research_args) => {
                    telemetry::typed_error(
                        "ERR_WEAK_TOOL_FOR_TURN",
                        "turn_policy",
                        "rerouted weak tool to research_web",
                        json!({
                            "requested_tool": name.clone(),
                            "task": task.chars().take(240).collect::<String>(),
                            "intent": intent.chars().take(240).collect::<String>(),
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
            let ctx = format!(
                "task: {task}; agent intent: {}",
                if intent.is_empty() {
                    "(none stated)"
                } else {
                    intent.as_str()
                }
            );
            let action_result = brain.dispatch(&dispatch_name, &dispatch_args, &ctx, &cancel);
            match brain.ground(&dispatch_name, &dispatch_args) {
                Ok(g) => {
                    let mut resp =
                        json!({"action_result": action_result, "new_state": g.state_text});
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
                            "screen_change" | "ui_change" | "stuck_warning" | "postcondition_block"
                        )
                    });
                    if no_effect {
                        let repeated = g.notes.iter().any(|(k, _)| *k == "postcondition_block");
                        resp["postcondition"] = json!({
                            "ok": false,
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
                    } else {
                        resp["postcondition"] = json!({
                            "ok": true,
                            "effect": "changed_or_not_applicable",
                        });
                        telemetry::event(
                            "postcondition",
                            "action_worker",
                            Privacy::Safe,
                            json!({"tool": dispatch_name.clone(), "ok": true}),
                        );
                    }
                    // On a detected stall, spend ONE grounded vision call (the merge
                    // planner) to propose a concrete next action — perception + plan in
                    // one shot, replacing the generic stuck warning with what it SEES.
                    if g.notes.iter().any(|(k, _)| *k == "stuck_warning")
                        && let Some(advice) = brain.stuck_advice(&task, &cancel)
                    {
                        resp["stuck_advice"] = json!(advice);
                    }
                    (id, name, resp, Some(g.frame_b64))
                }
                Err(e) => (
                    id,
                    name,
                    json!({"action_result": action_result, "ground_error": e.to_string()}),
                    None,
                ),
            }
        };
        if tx.send(done).is_err() {
            break;
        }
    }
}
