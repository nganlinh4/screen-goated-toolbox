use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

fn fresh_session() -> u64 {
    let sessions = crate::overlay::text_input::state::EditorSessions::default();
    sessions.begin();
    sessions.current().unwrap()
}

fn operation(nonce: &str) -> Arc<Operation> {
    Operation::new(fresh_session(), nonce.into(), None, Arc::new(|_| {})).unwrap()
}

#[test]
fn cancellation_before_admission_does_not_reach_renderer() {
    let op = operation("before");
    assert!(!op.cancel());
    assert!(!op.admit());
    assert_eq!(op.settlement(), Some(Settlement::Cancelled));
    assert!(get(op.id()).is_none());
}

#[test]
fn skipped_operation_quiesces_without_claiming_a_mutation() {
    let op = operation("skipped");
    assert!(op.admit());
    assert_eq!(
        op.ack_apply(ApplyResult::Skipped),
        Some(Settlement::Skipped)
    );
    assert!(get(op.id()).is_none());
}

#[test]
fn retired_session_waits_for_all_operations_even_after_uncertainty() {
    let session = fresh_session();
    let first = Operation::new(session, "session-barrier".into(), None, Arc::new(|_| {})).unwrap();
    let second = Operation::new(session, "session-barrier".into(), None, Arc::new(|_| {})).unwrap();
    assert!(first.admit());
    assert!(second.admit());
    let completed = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&completed);
    let _observation = observe_operations_quiescence(
        vec![Arc::clone(&first), Arc::clone(&second)],
        Arc::new(move || {
            seen.fetch_add(1, Ordering::SeqCst);
        }),
    );
    first.ack_apply(ApplyResult::Applied);
    second.fail_uncertain();
    assert_eq!(completed.load(Ordering::SeqCst), 0);
    second.ack_apply(ApplyResult::Applied);
    assert_eq!(completed.load(Ordering::SeqCst), 1);
}

#[test]
fn abandoned_activation_unsubscribes_from_uncertain_operations() {
    let op = operation("unsubscribed");
    assert!(op.admit());
    op.fail_uncertain();
    let completed = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&completed);
    let observation = observe_operations_quiescence(
        vec![Arc::clone(&op)],
        Arc::new(move || {
            seen.fetch_add(1, Ordering::SeqCst);
        }),
    );
    assert_eq!(op.state.lock().unwrap().quiescence_observers.len(), 1);
    drop(observation);
    assert!(op.state.lock().unwrap().quiescence_observers.is_empty());
    op.ack_apply(ApplyResult::Applied);
    assert_eq!(completed.load(Ordering::SeqCst), 0);
}

#[test]
fn retiring_prior_operations_cancels_unadmitted_and_waits_for_admitted() {
    let session = fresh_session();
    let queued = Operation::new(session, "prior".into(), None, Arc::new(|_| {})).unwrap();
    let revokes = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&revokes);
    let admitted = Operation::new(
        session,
        "prior".into(),
        None,
        Arc::new(move |_| {
            seen.fetch_add(1, Ordering::SeqCst);
        }),
    )
    .unwrap();
    assert!(admitted.admit());
    let completed = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&completed);
    let _observation = cancel_and_observe_operations(
        vec![Arc::clone(&queued), Arc::clone(&admitted)],
        Arc::new(move || {
            seen.fetch_add(1, Ordering::SeqCst);
        }),
    );
    assert_eq!(queued.settlement(), Some(Settlement::Cancelled));
    assert_eq!(revokes.load(Ordering::SeqCst), 1);
    assert_eq!(completed.load(Ordering::SeqCst), 0);
    admitted.ack_apply(ApplyResult::Rejected);
    admitted.ack_revoke(true);
    assert_eq!(completed.load(Ordering::SeqCst), 1);
}

#[test]
fn queue_sequence_is_nonzero_and_cannot_be_reassigned() {
    let op = operation("sequence");
    assert_eq!(op.sequence(), None);
    assert!(!op.set_sequence(0));
    assert!(op.set_sequence(7));
    assert!(!op.set_sequence(8));
    assert_eq!(op.sequence(), Some(7));
    op.cancel();
}

#[test]
fn cancelled_operations_wait_for_both_acknowledgments_in_either_order() {
    for revoke_first in [false, true] {
        let op = operation("order");
        assert!(op.admit());
        assert!(op.cancel());
        let (first, second) = if revoke_first {
            (op.ack_revoke(true), op.ack_apply(ApplyResult::Rejected))
        } else {
            (op.ack_apply(ApplyResult::Rejected), op.ack_revoke(true))
        };
        assert_eq!(first, None);
        assert_eq!(second, Some(Settlement::Cancelled));
        assert!(get(op.id()).is_none());
    }
}

#[test]
fn uncertain_work_retains_capacity_and_can_still_request_revocation() {
    let op = operation("uncertain");
    assert!(op.admit());
    assert_eq!(op.fail_uncertain(), Some(Settlement::Uncertain));
    assert!(get(op.id()).is_some());
    assert!(op.cancel());
    assert_eq!(op.ack_revoke(true), Some(Settlement::Uncertain));
    retire_document("uncertain");
    assert!(get(op.id()).is_none());
}

#[test]
fn timed_out_operation_is_swept_once_and_never_retried() {
    let op = operation("deadline");
    assert!(op.admit());
    let deadline = Instant::now() + ACK_TIMEOUT;
    assert!(expired(deadline).iter().any(|item| item.id() == op.id()));
    assert_eq!(op.fail_uncertain(), Some(Settlement::Uncertain));
    assert!(!expired(deadline).iter().any(|item| item.id() == op.id()));
    assert!(!op.admit());
    assert!(get(op.id()).is_some());
    retire_document("deadline");
}

#[test]
fn late_definitive_acknowledgments_release_uncertain_capacity() {
    let op = operation("late");
    let quiet = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&quiet);
    op.observe_quiescence(Arc::new(move || {
        seen.fetch_add(1, Ordering::SeqCst);
    }));
    assert!(op.admit());
    assert_eq!(op.fail_uncertain(), Some(Settlement::Uncertain));
    assert_eq!(quiet.load(Ordering::SeqCst), 0);
    assert!(op.cancel());
    assert_eq!(
        op.ack_apply(ApplyResult::Rejected),
        Some(Settlement::Uncertain)
    );
    assert!(get(op.id()).is_some());
    assert_eq!(op.ack_revoke(true), Some(Settlement::Uncertain));
    assert!(get(op.id()).is_none());
    assert_eq!(quiet.load(Ordering::SeqCst), 1);
}

#[test]
fn token_cancellation_fences_ack_even_before_its_callback_runs() {
    let token = ChainCancelToken::new();
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let release_rx = Mutex::new(release_rx);
    let blocker: CancelCallback = Arc::new(move || {
        entered_tx.send(()).unwrap();
        release_rx.lock().unwrap().recv().unwrap();
    });
    token.register_cancel_callback(&blocker);
    let revocations = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&revocations);
    let op = Operation::new(
        fresh_session(),
        "fence".into(),
        Some(&token),
        Arc::new(move |_| {
            seen.fetch_add(1, Ordering::SeqCst);
        }),
    )
    .unwrap();
    assert!(op.admit());
    let cancelling = Arc::clone(&token);
    let thread = std::thread::spawn(move || cancelling.cancel());
    entered_rx.recv().unwrap();
    assert_eq!(op.ack_apply(ApplyResult::Rejected), None);
    assert_eq!(revocations.load(Ordering::SeqCst), 1);
    assert_eq!(op.ack_revoke(true), Some(Settlement::Cancelled));
    release_tx.send(()).unwrap();
    thread.join().unwrap();
    assert_eq!(revocations.load(Ordering::SeqCst), 1);
}

#[test]
fn token_cancellation_fences_admission_before_callback_runs() {
    let token = ChainCancelToken::new();
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let release_rx = Mutex::new(release_rx);
    let blocker: CancelCallback = Arc::new(move || {
        entered_tx.send(()).unwrap();
        release_rx.lock().unwrap().recv().unwrap();
    });
    token.register_cancel_callback(&blocker);
    let op = Operation::new(
        fresh_session(),
        "admit-fence".into(),
        Some(&token),
        Arc::new(|_| {}),
    )
    .unwrap();
    let cancelling = Arc::clone(&token);
    let thread = std::thread::spawn(move || cancelling.cancel());
    entered_rx.recv().unwrap();
    assert!(!op.admit());
    assert_eq!(op.settlement(), Some(Settlement::Cancelled));
    assert!(get(op.id()).is_none());
    release_tx.send(()).unwrap();
    thread.join().unwrap();
}

#[test]
fn token_ancestors_revoke_once_and_observers_can_reenter() {
    let root = ChainCancelToken::new();
    let child = ChainCancelToken::child(&root);
    let revocations = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&revocations);
    let op = Operation::new(
        fresh_session(),
        "ancestor".into(),
        Some(&child),
        Arc::new(move |_| {
            seen.fetch_add(1, Ordering::SeqCst);
        }),
    )
    .unwrap();
    let observed = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&observed);
    let weak = Arc::downgrade(&op);
    op.observe(Arc::new(move |_| {
        assert!(weak.upgrade().unwrap().settlement().is_some());
        seen.fetch_add(1, Ordering::SeqCst);
    }));
    assert!(op.admit());
    root.cancel();
    child.cancel();
    assert_eq!(revocations.load(Ordering::SeqCst), 1);
    assert_eq!(op.ack_apply(ApplyResult::Applied), None);
    assert_eq!(
        op.ack_revoke(true),
        Some(Settlement::AppliedBeforeCancellationSettled)
    );
    assert_eq!(observed.load(Ordering::SeqCst), 1);
    assert!(get(op.id()).is_none());
}
