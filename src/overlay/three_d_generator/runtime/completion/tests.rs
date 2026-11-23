use super::*;
use crate::overlay::three_d_generator::runtime::{RuntimeOperation, idle_status};

fn pending_state() -> RuntimeState {
    let mut state = RuntimeState::default();
    let mut continuation = super::super::tests::continuation();
    continuation.dispatch_id = "dispatch".to_string();
    let operation = RuntimeOperation::Segment { continuation };
    let mut pending = idle_status();
    pending.stage = "finalizing".to_string();
    let mut completed = pending.clone();
    completed.stage = "done".to_string();
    completed.output_path = Some("model.glb".to_string());
    state.insert_job("job".to_string(), pending);
    state.operations.insert("job".to_string(), operation);
    state
        .pending_completions
        .insert("job".to_string(), (completed, None));
    state
}

#[test]
fn delivery_wait_does_not_consume_a_generator_slot() {
    let mut state = pending_state();
    assert_eq!(state.running_count(), 0);
    assert_eq!(state.pending_count(), 1);
    let mut active = idle_status();
    active.stage = "finalizing".to_string();
    state.insert_job("active".to_string(), active);
    assert_eq!(state.running_count(), 1);
}

#[test]
fn only_confirmed_delivery_becomes_done() {
    let mut state = pending_state();
    assert!(matches!(
        settle(
            &mut state,
            "job",
            "dispatch",
            "model.glb",
            DeliveryOutcome::Pending,
            None
        ),
        Settlement::Unchanged
    ));
    assert_eq!(state.jobs["job"].stage, "finalizing");
    assert!(matches!(
        settle(
            &mut state,
            "job",
            "dispatch",
            "model.glb",
            DeliveryOutcome::Delivered,
            None
        ),
        Settlement::Delivered
    ));
    assert_eq!(state.jobs["job"].stage, "done");
    assert!(!state.pending_completions.contains_key("job"));
    assert!(!state.operations.contains_key("job"));
}

#[test]
fn unjournaled_completion_fails_without_publishing_the_result() {
    let mut state = pending_state();
    let mut failed = idle_status();
    failed.stage = "failed".to_string();
    assert!(matches!(
        settle(
            &mut state,
            "job",
            "dispatch",
            "model.glb",
            DeliveryOutcome::Missing,
            Some(failed)
        ),
        Settlement::Failed(None)
    ));
    assert_eq!(state.jobs["job"].stage, "failed");
    assert!(state.jobs["job"].output_path.is_none());
    assert!(!state.pending_completions.contains_key("job"));
}

#[test]
fn stale_or_cancelled_jobs_cannot_be_resurrected() {
    let mut state = pending_state();
    assert!(matches!(
        settle(
            &mut state,
            "job",
            "other",
            "model.glb",
            DeliveryOutcome::Delivered,
            None
        ),
        Settlement::Unchanged
    ));
    state.jobs.get_mut("job").unwrap().stage = "cancelled".to_string();
    assert!(matches!(
        settle(
            &mut state,
            "job",
            "dispatch",
            "model.glb",
            DeliveryOutcome::Delivered,
            None
        ),
        Settlement::Unchanged
    ));
    assert_eq!(state.jobs["job"].stage, "cancelled");
}
