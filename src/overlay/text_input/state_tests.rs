use super::*;

#[test]
fn nonblocking_editor_capture_distinguishes_contention_from_no_session() {
    let sessions = EditorSessions::default();
    assert_eq!(sessions.try_current().unwrap(), None);
    sessions.begin();
    let session = sessions.current().unwrap();
    let held = sessions.state.lock().unwrap();
    assert!(sessions.try_current().is_err());
    drop(held);
    assert_eq!(sessions.try_current().unwrap(), Some(session));
    sessions.end();
    assert_eq!(sessions.try_current().unwrap(), None);
}
use crate::overlay::result::ChainCancelToken;
use std::cell::Cell;
use std::sync::atomic::Ordering;

#[test]
fn submit_can_replace_callback_without_lock_reentry() {
    let slot: Arc<Mutex<Option<TextSubmitCallback>>> = Arc::new(Mutex::new(None));
    let invoked = Arc::new(AtomicU64::new(0));
    let replacement_slot = slot.clone();
    let replacement_invoked = invoked.clone();
    *slot.lock().unwrap() = Some(Arc::new(move |_, _, _| {
        assert!(SUBMISSION_IN_PROGRESS.with(|active| active.get()));
        let invoked = replacement_invoked.clone();
        *replacement_slot.lock().unwrap() = Some(Arc::new(move |_, _, _| {
            invoked.fetch_add(1, Ordering::SeqCst);
        }));
    }));
    dispatch_submit(&slot, "first".into(), HWND::default(), 1);
    assert!(!SUBMISSION_IN_PROGRESS.with(|active| active.get()));
    dispatch_submit(&slot, "second".into(), HWND::default(), 1);
    assert_eq!(invoked.load(Ordering::SeqCst), 1);
}

#[test]
fn session_queue_preserves_order_and_rejects_stale_producers() {
    let sessions = EditorSessions::default();
    sessions.begin();
    let old = sessions.current().unwrap();
    assert!(sessions.enqueue(PendingText {
        text: "old".into(),
        action: EditorAction::Insert,
        session: old,
        owner: None,
        operation: None
    }));
    sessions.begin();
    let current = sessions.current().unwrap();
    assert!(!sessions.enqueue(PendingText {
        text: "late".into(),
        action: EditorAction::Insert,
        session: old,
        owner: None,
        operation: None
    }));
    assert!(sessions.enqueue(PendingText {
        text: "first".into(),
        action: EditorAction::Insert,
        session: current,
        owner: None,
        operation: None
    }));
    assert!(sessions.enqueue(PendingText {
        text: "second".into(),
        action: EditorAction::Insert,
        session: current,
        owner: None,
        operation: None
    }));
    assert_eq!(sessions.take_pending().unwrap().text, "first");
    assert_eq!(sessions.take_pending().unwrap().text, "second");
    assert!(sessions.take_pending().is_none());
}

#[test]
fn session_queue_rejects_overflow_and_clears_on_retirement() {
    let sessions = EditorSessions::default();
    sessions.begin();
    let session = sessions.current().unwrap();
    assert!(!sessions.enqueue(PendingText {
        text: "x".repeat(MAX_PENDING_TEXT_BYTES + 1),
        action: EditorAction::Insert,
        session,
        owner: None,
        operation: None
    }));
    for _ in 0..MAX_PENDING_TEXT_ITEMS {
        assert!(sessions.enqueue(PendingText {
            text: "x".into(),
            action: EditorAction::Insert,
            session,
            owner: None,
            operation: None
        }));
    }
    assert!(!sessions.enqueue(PendingText {
        text: "overflow".into(),
        action: EditorAction::Insert,
        session,
        owner: None,
        operation: None
    }));
    sessions.end();
    assert!(sessions.take_pending().is_none());
    assert!(!sessions.enqueue(PendingText {
        text: "late".into(),
        action: EditorAction::Insert,
        session,
        owner: None,
        operation: None
    }));
}

#[test]
fn clear_snapshot_counts_toward_queue_budget_and_is_released() {
    let sessions = EditorSessions::default();
    sessions.begin();
    let session = sessions.current().unwrap();
    let clear = |size| PendingText {
        text: String::new(),
        action: EditorAction::Clear {
            revision: "1".into(),
            passive: true,
            snapshot: "x".repeat(size),
        },
        session,
        owner: None,
        operation: None,
    };
    assert!(!sessions.enqueue(clear(MAX_PENDING_TEXT_BYTES)));
    assert!(sessions.enqueue(clear(MAX_PENDING_TEXT_BYTES - 1)));
    assert!(!sessions.enqueue(clear(1)));
    sessions.take_pending().unwrap();
    assert!(sessions.enqueue(clear(1)));
    sessions.end();
}

#[test]
fn queue_sequence_follows_enqueue_not_operation_creation_order() {
    let sessions = EditorSessions::default();
    sessions.begin();
    let session = sessions.current().unwrap();
    let revoke: Arc<dyn Fn(u64) + Send + Sync> = Arc::new(|_| {});
    let first_created = super::super::delivery::Operation::new(
        session,
        "sequence-test".into(),
        None,
        revoke.clone(),
    )
    .unwrap();
    let second_created =
        super::super::delivery::Operation::new(session, "sequence-test".into(), None, revoke)
            .unwrap();
    assert!(first_created.id() < second_created.id());
    assert!(sessions.enqueue(PendingText {
        text: "second-created".into(),
        action: EditorAction::Insert,
        session,
        owner: None,
        operation: Some(second_created.clone())
    }));
    assert!(sessions.enqueue(PendingText {
        text: "first-created".into(),
        action: EditorAction::Insert,
        session,
        owner: None,
        operation: Some(first_created.clone())
    }));
    assert_eq!(second_created.sequence(), Some(1));
    assert_eq!(first_created.sequence(), Some(2));
    assert_eq!(sessions.take_pending().unwrap().text, "second-created");
    assert!(sessions.take_pending().is_none());
    assert!(sessions.finish_operation(second_created.id()));
    assert_eq!(sessions.take_pending().unwrap().text, "first-created");
    second_created.cancel();
    first_created.cancel();
    sessions.end();
}

#[test]
fn queued_text_rechecks_ancestor_cancellation_at_dispatch() {
    let sessions = EditorSessions::default();
    sessions.begin();
    let root = ChainCancelToken::new();
    let child = ChainCancelToken::child(&root);
    let pending = PendingText {
        text: "hello".into(),
        action: EditorAction::Insert,
        session: sessions.current().unwrap(),
        operation: None,
        owner: Some(EditorOwner {
            token: child,
            session: sessions.current().unwrap(),
        }),
    };
    root.cancel();
    let invoked = Cell::new(false);
    pending.dispatch(&sessions, |_, _, _, _| invoked.set(true));
    assert!(!invoked.get());
}

#[test]
fn unowned_text_is_dispatched() {
    let sessions = EditorSessions::default();
    sessions.begin();
    let text = RefCell::new(String::new());
    PendingText {
        text: "history".into(),
        action: EditorAction::Replace,
        session: sessions.current().unwrap(),
        operation: None,
        owner: None,
    }
    .dispatch(&sessions, |value, action, owner, _| {
        assert!(owner.is_none());
        assert_eq!(action, EditorAction::Replace);
        *text.borrow_mut() = value;
    });
    assert_eq!(*text.borrow(), "history");
}

#[test]
fn literal_marker_text_remains_an_insertion() {
    let sessions = EditorSessions::default();
    sessions.begin();
    let pending = PendingText {
        text: "__REPLACE_ALL__literal".into(),
        action: EditorAction::Insert,
        session: sessions.current().unwrap(),
        owner: None,
        operation: None,
    };
    let called = Cell::new(false);
    pending.dispatch(&sessions, |text, action, _, _| {
        assert_eq!(text, "__REPLACE_ALL__literal");
        assert_eq!(action, EditorAction::Insert);
        called.set(true);
    });
    assert!(called.get());
}

#[test]
fn reopened_editor_rejects_previous_session_text() {
    let sessions = EditorSessions::default();
    sessions.begin();
    let owner = EditorOwner {
        token: ChainCancelToken::new(),
        session: sessions.current().unwrap(),
    };
    let pending = PendingText {
        text: "old".into(),
        action: EditorAction::Insert,
        session: owner.session,
        operation: None,
        owner: Some(owner.clone()),
    };
    sessions.end();
    assert_eq!(sessions.current(), None);
    sessions.begin();
    assert_ne!(sessions.current(), Some(owner.session));
    let inserted = Cell::new(false);
    pending.dispatch(&sessions, |_, _, _, _| inserted.set(true));
    assert!(!inserted.get());
}
