use super::*;
use std::cell::Cell;
use std::cell::RefCell;

#[test]
fn page_drain_precedes_removal_and_failure_prevents_it() {
    let events = RefCell::new(Vec::new());
    let result = with_page_drain(
        || {
            events.borrow_mut().push("queued edit applied");
            events.borrow_mut().push("page acknowledged");
            Ok(())
        },
        || {
            events.borrow_mut().push("flush and remove");
            Ok(())
        },
    );
    assert!(result.is_ok());
    assert_eq!(
        *events.borrow(),
        [
            "queued edit applied",
            "page acknowledged",
            "flush and remove"
        ]
    );

    events.borrow_mut().clear();
    let failed: anyhow::Result<()> = with_page_drain(
        || Err::<(), _>(std::io::Error::other("page did not acknowledge")),
        || {
            events.borrow_mut().push("remove");
            Ok(())
        },
    );
    assert!(failed.is_err());
    assert!(events.borrow().is_empty());
}

#[test]
fn failed_flush_posts_once_and_reports_failure() {
    let posts = Cell::new(0);
    let failed = Cell::new(false);
    let result = wait_for_removal(
        Instant::now() + Duration::from_secs(1),
        || (Some(42), false),
        |_| {
            posts.set(posts.get() + 1);
            failed.set(true);
            Ok(())
        },
        || failed.get(),
        || {},
    );
    assert!(result.is_err());
    assert_eq!(posts.get(), 1);
}

#[test]
fn waits_for_initialization_before_posting_removal() {
    let observations = Cell::new(0);
    let posts = Cell::new(0);
    let result = wait_for_removal(
        Instant::now() + Duration::from_secs(1),
        || {
            let count = observations.get();
            observations.set(count + 1);
            match count {
                0 => (Some(42), true),
                1 => (Some(42), false),
                _ => (None, false),
            }
        },
        |_| {
            assert!(observations.get() >= 2);
            posts.set(posts.get() + 1);
            Ok(())
        },
        || false,
        || {},
    );
    assert!(result.is_ok());
    assert_eq!(posts.get(), 1);
}
