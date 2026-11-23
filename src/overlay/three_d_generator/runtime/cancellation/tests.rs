use super::*;
use std::sync::mpsc;
use std::time::Duration;

fn isolated(name: &str, run: impl FnOnce()) {
    let name = format!("{}::{name}", module_path!().split_once("::").unwrap().1);
    crate::overlay::creation_submission_test::run_isolated(&name, |_| run());
}

fn setup() {
    let mut state = STATE.lock().unwrap();
    *state = RuntimeState::default();
    let mut continuation = super::super::tests::continuation();
    continuation.dispatch_id = "dispatch".into();
    state
        .operations
        .insert("job".into(), RuntimeOperation::Segment { continuation });
    state
        .request_fingerprints
        .insert("job".into(), "fingerprint".into());
    state.deadlines.insert("job".into(), u64::MAX);
    state.job_order.push("job".into());
    let mut status = idle_status();
    status.job_id = Some("job".into());
    status.stage = "generating".into();
    state.jobs.insert("job".into(), status);
    state.pids.insert("job".into(), 42);
}

#[test]
fn failed_cancel_preserves_concurrent_progress_and_pending_completion() {
    isolated(
        "failed_cancel_preserves_concurrent_progress_and_pending_completion",
        || {
            setup();
            std::thread::scope(|scope| {
                let (entered, waiting) = mpsc::channel();
                let (release, resume) = mpsc::channel();
                let cancellation = scope.spawn(move || {
                    cancel_jobs_with(
                        Some("job"),
                        false,
                        |_| {
                            entered.send(()).unwrap();
                            resume.recv_timeout(Duration::from_secs(3)).unwrap();
                            Err("storage unavailable".into())
                        },
                        |_| panic!("non-durable cancellation killed a worker"),
                    )
                });
                waiting.recv_timeout(Duration::from_secs(3)).unwrap();
                assert!(!super::super::recovery::job_cancelled("job"));
                super::super::process::update_progress(
                    "job",
                    &serde_json::json!({"stage":"generating","progressRatio":0.5}),
                    "installed",
                );
                assert_eq!(STATE.lock().unwrap().jobs["job"].progress_ratio, Some(0.5));
                let mut completed = STATE.lock().unwrap().jobs["job"].clone();
                completed.output_path = Some("model.glb".into());
                super::super::process::finish_job_pending("job", completed, None);
                release.send(()).unwrap();
                let (durable, status) = cancellation.join().unwrap();
                assert!(!durable);
                assert_eq!(status.stage, "finalizing");
                let state = STATE.lock().unwrap();
                assert!(state.pending_completions.contains_key("job"));
                assert!(state.operations.contains_key("job"));
                assert!(!state.pids.contains_key("job"));
            });
        },
    );
}

#[test]
fn failed_cancel_preserves_a_concurrent_terminal_failure() {
    isolated(
        "failed_cancel_preserves_a_concurrent_terminal_failure",
        || {
            setup();
            let (durable, status) = cancel_jobs_with(
                Some("job"),
                false,
                |_| {
                    let mut failed = STATE.lock().unwrap().jobs["job"].clone();
                    failed.stage = "failed".into();
                    super::super::process::finish_job_retaining_intent("job", failed, None);
                    Err("storage unavailable".into())
                },
                |_| panic!("failed cancellation killed a retired worker"),
            );
            assert!(!durable);
            assert_eq!(status.stage, "failed");
            assert!(!STATE.lock().unwrap().operations.contains_key("job"));
        },
    );
}

#[test]
fn durable_cancellation_rejects_late_completion_and_retires_only_owned_execution() {
    isolated(
        "durable_cancellation_rejects_late_completion_and_retires_only_owned_execution",
        || {
            setup();
            let mut terminated = vec![];
            let (durable, status) = cancel_jobs_with(
                Some("job"),
                false,
                |_| Ok(CancellationOutcome::Cancelled),
                |pid| terminated.push(pid.pid()),
            );
            assert!(durable);
            assert_eq!(status.stage, "cancelled");
            assert_eq!(terminated, [42]);
            assert!(super::super::recovery::job_cancelled("job"));
            let mut completed = status;
            completed.stage = "done".into();
            super::super::process::finish_job_pending("job", completed.clone(), None);
            super::super::process::finish_job_retaining_intent("job", completed, None);
            let state = STATE.lock().unwrap();
            assert_eq!(state.jobs["job"].stage, "cancelled");
            assert!(state.pending_completions.is_empty());
            assert!(state.operations.is_empty());
            assert!(state.pids.is_empty());
        },
    );
}

#[test]
fn a_published_winner_is_never_cancelled_or_terminated() {
    isolated(
        "a_published_winner_is_never_cancelled_or_terminated",
        || {
            setup();
            let (durable, status) = cancel_jobs_with(
                Some("job"),
                false,
                |_| {
                    let mut completed = STATE.lock().unwrap().jobs["job"].clone();
                    completed.stage = "done".into();
                    super::super::process::finish_job_retaining_intent("job", completed, None);
                    Ok(CancellationOutcome::Published)
                },
                |_| panic!("publication winner was terminated"),
            );
            assert!(durable);
            assert_eq!(status.stage, "done");
        },
    );
}

#[test]
fn stale_cancellation_cannot_cancel_a_new_dispatch_in_the_same_slot() {
    isolated(
        "stale_cancellation_cannot_cancel_a_new_dispatch_in_the_same_slot",
        || {
            setup();
            let (_, status) = cancel_jobs_with(
                Some("job"),
                false,
                |_| {
                    let mut state = STATE.lock().unwrap();
                    if let RuntimeOperation::Segment { continuation } =
                        state.operations.get_mut("job").unwrap()
                    {
                        continuation.dispatch_id = "new-dispatch".into();
                    }
                    state
                        .request_fingerprints
                        .insert("job".into(), "new-fingerprint".into());
                    Ok(CancellationOutcome::Cancelled)
                },
                |_| panic!("stale cancellation killed a new dispatch"),
            );
            assert_eq!(status.stage, "generating");
            assert_eq!(STATE.lock().unwrap().pids["job"], 42);
        },
    );
}

#[test]
fn shutdown_keeps_failed_persistence_pending_while_stopping_execution() {
    isolated(
        "shutdown_keeps_failed_persistence_pending_while_stopping_execution",
        || {
            setup();
            let mut terminated = vec![];
            let (durable, status) = cancel_jobs_with(
                None,
                true,
                |_| Err("storage unavailable".into()),
                |pid| terminated.push(pid.pid()),
            );
            assert!(!durable);
            assert_eq!(status.stage, "cancelling");
            assert_eq!(terminated, [42]);
            let state = STATE.lock().unwrap();
            assert!(state.operations.contains_key("job"));
            assert!(state.request_fingerprints.contains_key("job"));
            assert!(state.pids.is_empty());
        },
    );
}

/// Installs job "parent-id" at `stage` with a real source snapshot whose
/// continuation retention is owned by that job; returns that continuation.
fn retained_job(root: &std::path::Path, stage: &str) -> Continuation {
    super::super::tests::prepare_refinement(root, "balanced");
    let retained = STATE
        .lock()
        .unwrap()
        .continuations
        .remove("parent-id")
        .unwrap();
    let mut state = STATE.lock().unwrap();
    *state = RuntimeState::default();
    let mut continuation = super::super::tests::continuation();
    continuation.dispatch_id = "dispatch".into();
    state.operations.insert(
        "parent-id".into(),
        RuntimeOperation::Segment { continuation },
    );
    state
        .request_fingerprints
        .insert("parent-id".into(), "fingerprint".into());
    state.deadlines.insert("parent-id".into(), u64::MAX);
    state.job_order.push("parent-id".into());
    let mut status = idle_status();
    status.job_id = Some("parent-id".into());
    status.stage = stage.into();
    state.jobs.insert("parent-id".into(), status);
    retained
}

/// Another continuation can own the source only after the job's retention
/// was released.
fn assert_released(retained: &Continuation) {
    crate::overlay::creation_source_snapshot::retain_continuation(
        std::slice::from_ref(&retained.source_descriptor),
        "parent-dispatch",
        "replacement-id",
        retained.expires_at_ms,
    )
    .unwrap();
}

fn isolated_root(name: &str, run: impl FnOnce(&std::path::Path)) {
    let name = format!("{}::{name}", module_path!().split_once("::").unwrap().1);
    crate::overlay::creation_submission_test::run_isolated(&name, run);
}

#[test]
fn durable_cancel_releases_a_pending_completion_continuation() {
    isolated_root(
        "durable_cancel_releases_a_pending_completion_continuation",
        |root| {
            let retained = retained_job(root, "finalizing");
            let mut completed = STATE.lock().unwrap().jobs["parent-id"].clone();
            completed.stage = "done".into();
            STATE
                .lock()
                .unwrap()
                .pending_completions
                .insert("parent-id".into(), (completed, Some(retained.clone())));
            let (durable, status) = cancel_jobs_with(
                Some("parent-id"),
                false,
                |_| Ok(CancellationOutcome::Cancelled),
                |_| {},
            );
            assert!(durable);
            assert_eq!(status.stage, "cancelled");
            assert_released(&retained);
        },
    );
}

#[test]
fn late_completion_after_durable_cancel_releases_its_continuation() {
    isolated_root(
        "late_completion_after_durable_cancel_releases_its_continuation",
        |root| {
            let retained = retained_job(root, "cancelled");
            let mut completed = STATE.lock().unwrap().jobs["parent-id"].clone();
            completed.stage = "done".into();
            super::super::process::finish_job_pending(
                "parent-id",
                completed,
                Some(retained.clone()),
            );
            assert_eq!(STATE.lock().unwrap().jobs["parent-id"].stage, "cancelled");
            assert_released(&retained);
        },
    );
}

#[test]
fn late_terminal_completion_after_durable_cancel_releases_its_continuation() {
    isolated_root(
        "late_terminal_completion_after_durable_cancel_releases_its_continuation",
        |root| {
            let retained = retained_job(root, "cancelled");
            let mut completed = STATE.lock().unwrap().jobs["parent-id"].clone();
            completed.stage = "done".into();
            super::super::process::finish_job("parent-id", completed, Some(retained.clone()));
            assert_eq!(STATE.lock().unwrap().jobs["parent-id"].stage, "cancelled");
            assert!(
                !STATE
                    .lock()
                    .unwrap()
                    .continuations
                    .contains_key("parent-id")
            );
            assert_released(&retained);
        },
    );
}
