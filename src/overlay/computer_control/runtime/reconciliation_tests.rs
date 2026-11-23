use super::*;
use std::sync::atomic::AtomicBool;

fn pending(id: &str, token: &Arc<AtomicBool>) -> Pending {
    Pending {
        id: Some(id.into()),
        cancel: Some(token.clone()),
        ..Pending::default()
    }
}

#[test]
fn only_matching_job_and_token_discharge_trusted_no_dispatch() {
    let token = Arc::new(AtomicBool::new(false));
    let other = Arc::new(AtomicBool::new(false));
    let mut obligation = Reconciliation::default();
    assert!(!obligation.require_pending(&pending("one", &token)));
    assert!(obligation.settle("other", &token, true, false).is_none());
    assert!(obligation.settle("one", &other, true, false).is_none());
    assert!(obligation.required());
    assert!(obligation.settle("one", &token, true, false).is_some());
    assert!(!obligation.required());
}

#[test]
fn unknown_and_multiple_sources_cannot_be_cleared_by_an_old_receipt() {
    let first = Arc::new(AtomicBool::new(false));
    let second = Arc::new(AtomicBool::new(false));
    let mut obligation = Reconciliation::default();
    obligation.require_pending(&pending("one", &first));
    assert!(obligation.require_pending(&pending("two", &second)));
    assert!(obligation.unresolved());
    assert!(obligation.settle("one", &first, true, false).is_none());
    assert!(obligation.required());
}

#[test]
fn accepted_no_dispatch_waits_for_external_quiescence() {
    let token = Arc::new(AtomicBool::new(false));
    let mut obligation = Reconciliation::default();
    obligation.require_pending(&pending("one", &token));
    assert!(obligation.settle("one", &token, true, true).is_none());
    assert!(obligation.discharge_if_quiet(true).is_none());
    assert!(obligation.discharge_if_quiet(false).is_some());
    assert!(!obligation.required());
}

#[test]
fn dispatched_or_unowned_work_stays_unresolved() {
    let token = Arc::new(AtomicBool::new(false));
    let mut obligation = Reconciliation::default();
    obligation.require_pending(&pending("one", &token));
    assert!(obligation.settle("one", &token, false, false).is_none());
    assert!(obligation.unresolved());
    assert!(obligation.settle("one", &token, true, false).is_none());
    let mut unowned = Reconciliation::default();
    assert!(unowned.require_pending(&Pending::default()));
    assert!(unowned.unresolved());
}
