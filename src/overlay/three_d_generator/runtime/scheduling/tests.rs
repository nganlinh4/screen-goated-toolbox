use super::super::tests::continuation;
use super::super::{
    MAX_PARALLEL_JOBS, RuntimeOperation, RuntimeState, idle_status, status_is_busy,
};

fn queue(state: &mut RuntimeState, job_id: &str, operation: RuntimeOperation) {
    let mut status = idle_status();
    status.job_id = Some(job_id.to_string());
    status.stage = "queued".to_string();
    state.insert_job(job_id.to_string(), status);
    state.operations.insert(job_id.to_string(), operation);
    state
        .request_fingerprints
        .insert(job_id.to_string(), format!("fingerprint-{job_id}"));
    state.deadlines.insert(job_id.to_string(), 7_000);
}

fn segment() -> RuntimeOperation {
    RuntimeOperation::Segment {
        continuation: continuation(),
    }
}

fn refine() -> RuntimeOperation {
    RuntimeOperation::Refine {
        continuation: continuation(),
    }
}

fn stage(state: &RuntimeState, job_id: &str) -> String {
    state.jobs[job_id].stage.clone()
}

#[test]
fn queued_jobs_dispatch_in_order_until_parallel_capacity_is_full() {
    let mut state = RuntimeState::default();
    for job_id in ["first", "second", "third"] {
        queue(&mut state, job_id, segment());
    }
    let first = state.claim_next_dispatch().unwrap();
    assert_eq!(first.job_id, "first");
    assert_eq!(first.request_fingerprint, "fingerprint-first");
    assert_eq!(first.deadline_at_ms, 7_000);
    assert!(!first.recovered);
    assert_eq!(stage(&state, "first"), "segmenting");
    assert_eq!(state.jobs["first"].phase.as_deref(), Some("separation"));
    assert_eq!(state.claim_next_dispatch().unwrap().job_id, "second");
    assert_eq!(state.running_count(), MAX_PARALLEL_JOBS);
    assert!(state.claim_next_dispatch().is_none());
    assert_eq!(stage(&state, "third"), "queued");
}

#[test]
fn a_claimed_job_is_never_dispatched_twice() {
    let mut state = RuntimeState::default();
    queue(&mut state, "only", refine());
    let claimed = state.claim_next_dispatch().unwrap();
    assert_eq!(claimed.job_id, "only");
    assert_eq!(stage(&state, "only"), "refining");
    assert!(status_is_busy(&stage(&state, "only")));
    assert!(state.claim_next_dispatch().is_none());
}

#[test]
fn queued_progress_with_a_live_process_keeps_its_slot() {
    let mut state = RuntimeState::default();
    queue(&mut state, "running-a", segment());
    queue(&mut state, "running-b", segment());
    queue(&mut state, "waiting", segment());
    // A runtime that reports "queued" progress still owns its process.
    state.pids.insert("running-a".to_string(), 11);
    state.pids.insert("running-b".to_string(), 12);
    assert!(state.claim_next_dispatch().is_none());
    assert_eq!(stage(&state, "waiting"), "queued");
}

#[test]
fn pending_completion_releases_its_slot_to_the_next_queued_job() {
    let mut state = RuntimeState::default();
    for job_id in ["a", "b", "waiting"] {
        queue(&mut state, job_id, segment());
    }
    state.claim_next_dispatch().unwrap();
    state.claim_next_dispatch().unwrap();
    assert!(state.claim_next_dispatch().is_none());
    let mut finished = state.jobs["a"].clone();
    finished.stage = "done".to_string();
    state
        .pending_completions
        .insert("a".to_string(), (finished, None));
    assert_eq!(state.claim_next_dispatch().unwrap().job_id, "waiting");
    assert_eq!(state.running_count(), MAX_PARALLEL_JOBS);
}

#[test]
fn recovered_marker_is_consumed_by_the_dispatch_that_resumes_the_job() {
    let mut state = RuntimeState::default();
    queue(&mut state, "resumed", segment());
    state.recovered_jobs.insert("resumed".to_string());
    assert!(state.claim_next_dispatch().unwrap().recovered);
    assert!(!state.recovered_jobs.contains("resumed"));
}

#[test]
fn incomplete_admission_records_are_not_dispatched() {
    let mut state = RuntimeState::default();
    queue(&mut state, "missing-deadline", segment());
    state.deadlines.remove("missing-deadline");
    assert!(state.claim_next_dispatch().is_none());
    assert_eq!(stage(&state, "missing-deadline"), "queued");
    assert_eq!(state.running_count(), 0);
}
