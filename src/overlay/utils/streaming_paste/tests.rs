use super::*;

#[test]
fn delayed_or_cancelled_capture_cannot_claim_a_target() {
    let start = Instant::now();
    let deadline = start + Duration::from_secs(2);
    assert!(capture_allowed(start, deadline, false));
    assert!(!capture_allowed(start, deadline, true));
    assert!(!capture_allowed(deadline, deadline, false));
}

#[test]
fn mailbox_coalesces_only_adjacent_interims_and_preserves_final_order() {
    let mut state = Mailbox::default();
    for event in [
        Event::Interim("old".into()),
        Event::Interim("new".into()),
        Event::Final("one".into()),
        Event::Interim("next".into()),
        Event::Final("two".into()),
    ] {
        enqueue(&mut state, event);
    }
    assert_eq!(
        state.queue.into_iter().collect::<Vec<_>>(),
        vec![
            Event::Interim("new".into()),
            Event::Final("one".into()),
            Event::Interim("next".into()),
            Event::Final("two".into())
        ]
    );
}

#[test]
fn mailbox_overflow_discards_work_instead_of_growing_or_replaying() {
    let mut state = Mailbox::default();
    enqueue(&mut state, Event::Final("x".repeat(MAX_TEXT_BYTES + 1)));
    assert!(state.overflow && state.queue.is_empty() && !state.accepting);
}

#[test]
fn disabled_auto_paste_never_claims_target_or_starts_worker() {
    let writer = StreamingAutoPaste::new(false, Arc::new(AtomicBool::new(false)));
    writer.interim("preview");
    writer.final_text("final");
    writer.finish();
    assert!(writer.shared.is_none());
}

#[test]
fn dispatch_permit_rechecks_drain_abort_and_expiration() {
    let interim = Event::Interim("draft".into());
    let final_text = Event::Final("final".into());
    let mut state = Mailbox::default();
    assert!(permits(&state, &interim, false));
    state.draining = true;
    assert!(!permits(&state, &interim, false));
    assert!(permits(&state, &final_text, false));
    assert!(!permits(&state, &final_text, true));
    assert!(permits(&state, &Event::Finish, true));
    state.overflow = true;
    assert!(!permits(&state, &final_text, false));
    assert!(permits(&state, &Event::Finish, false));
    state.expired = true;
    assert!(!permits(&state, &Event::Finish, true));
}
