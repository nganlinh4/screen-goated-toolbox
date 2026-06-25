//! The action-worker thread: owns the shared `Brain` and runs each tool call the
//! Live model emits to completion (humanized, cancellable), so a slow action can
//! glide while the session/reader thread keeps receiving mic + barge-in. It talks
//! to the session loop only through the `Job` / `Done` channels.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

use serde_json::json;

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
    while let Ok((id, name, args, task, intent)) = rx.recv() {
        cancel.store(false, Ordering::SeqCst); // each action starts fresh
        let done: Done = if name == "done" {
            // Independent high-res check - the Live agent confabulates success.
            let (ok, verdict) = brain.verify_done(&task, &cancel);
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
            let ctx = format!(
                "task: {task}; agent intent: {}",
                if intent.is_empty() { "(none stated)" } else { intent.as_str() }
            );
            let action_result = brain.dispatch(&name, &args, &ctx, &cancel);
            match brain.ground(&name, &args) {
                Ok(g) => {
                    let mut resp = json!({"action_result": action_result, "new_state": g.state_text});
                    for (k, v) in &g.notes {
                        resp[*k] = json!(*v);
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
                Err(e) => (id, name, json!({"action_result": action_result, "ground_error": e.to_string()}), None),
            }
        };
        if tx.send(done).is_err() {
            break;
        }
    }
}
