use super::capture::CapturedOutput;
use super::{CapturedChild, ProcessSpec};
use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const POLL_INTERVAL: Duration = Duration::from_millis(20);
const REAP_LIMIT: Duration = Duration::from_secs(5);

pub(super) enum WaitResult {
    Complete(CapturedOutput),
    Interrupted(Value),
}

pub(super) fn wait_cancellable(
    mut child: CapturedChild,
    timeout: Duration,
    cancel: &AtomicBool,
) -> Result<WaitResult> {
    if cancel.load(Ordering::SeqCst) {
        return Ok(WaitResult::Interrupted(stop(
            &mut child,
            "cancelled",
            false,
        )));
    }
    child.child.resume()?;
    let started = Instant::now();
    loop {
        if cancel.load(Ordering::SeqCst) {
            return Ok(WaitResult::Interrupted(stop(&mut child, "cancelled", true)));
        }
        if started.elapsed() >= timeout {
            return Ok(WaitResult::Interrupted(stop(&mut child, "timed_out", true)));
        }
        if let Some(status) = child.child.try_wait().context("wait for command process")? {
            // Check again before releasing the job: a cancellation owns any
            // descendants while this action is still in flight.
            if cancel.load(Ordering::SeqCst) {
                return Ok(WaitResult::Interrupted(stop(&mut child, "cancelled", true)));
            }
            let output = child.capture.finish(status, cancel);
            // The successful release is the command's terminal handoff. The
            // cancellation producer shares this lock, so cancellation either
            // wins before release or observes a command already completed.
            if super::super::job_cancellation::complete_unless_cancelled(cancel, || {
                child.child.release_descendants()
            })
            .transpose()?
            .is_none()
            {
                return Ok(WaitResult::Interrupted(stop(&mut child, "cancelled", true)));
            }
            return Ok(WaitResult::Complete(output));
        }
        std::thread::sleep(POLL_INTERVAL.min(timeout.saturating_sub(started.elapsed())));
    }
}

fn stop(child: &mut CapturedChild, reason: &str, started: bool) -> Value {
    let termination = child.child.terminate();
    let deadline = Instant::now() + REAP_LIMIT;
    let mut reaped = false;
    let mut drained = false;
    loop {
        if !reaped {
            reaped = matches!(child.child.try_wait(), Ok(Some(_)));
        }
        drained |= matches!(child.child.active_processes(), Ok(0));
        if reaped && drained || Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(POLL_INTERVAL);
    }
    interrupted_value(
        reason,
        started,
        reaped,
        drained,
        termination.err().map(|error| error.to_string()),
    )
}

pub(super) fn interrupted_value(
    reason: &str,
    started: bool,
    reaped: bool,
    drained: bool,
    termination_error: Option<String>,
) -> Value {
    json!({
        "ok": false,
        "status": if reason == "cancelled" { "aborted_by_user" } else { "timed_out" },
        "cancelled": reason == "cancelled",
        "timed_out": reason == "timed_out",
        "effect_verified": false,
        "effect_may_have_occurred": started,
        "termination_requested": termination_error.is_none(),
        "terminated": termination_error.is_none() && drained,
        "owned_processes_stopped": drained,
        "reaped": reaped,
        "termination_error": termination_error,
        "error": if reason == "cancelled" {
            "command cancellation requested"
        } else {
            "command deadline exceeded"
        },
    })
}

pub(super) fn process_interrupted_value(mut value: Value, spec: &ProcessSpec) -> Value {
    value["ok_scope"] = json!("exact_process_exit_status");
    value["evidence_kind"] = json!("exact_process_invocation");
    value["program"] = json!(spec.program);
    value["args"] = json!(spec.args);
    value["arg_count"] = json!(spec.args.len());
    value["cwd"] = json!(spec.cwd.to_string_lossy());
    value["completion_proof"] = json!({
        "exact": ["/program", "/args", "/arg_count", "/cwd", "/cancelled",
                  "/timed_out", "/termination_requested", "/terminated", "/reaped"]
    });
    value
}
