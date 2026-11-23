use super::*;
use std::cell::Cell;

#[test]
fn pending_reset_clears_before_saving_and_requires_ownership() {
    let step = Cell::new(0);
    process_pending(
        true,
        true,
        || {
            step.set(1);
            Ok(())
        },
        || {
            assert_eq!(step.get(), 1);
            step.set(2);
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(step.get(), 2);
    assert!(process_pending(true, false, || panic!("must not clear"), || Ok(())).is_err());
    assert!(
        process_pending(
            false,
            false,
            || panic!("must not clear"),
            || panic!("must not save")
        )
        .is_ok()
    );
}

#[test]
fn cleanup_and_save_failures_are_returned_without_false_success() {
    let saved = Cell::new(false);
    assert!(
        process_pending(
            true,
            true,
            || Err(io::Error::other("busy")),
            || {
                saved.set(true);
                Ok(())
            }
        )
        .is_err()
    );
    assert!(!saved.get());
    assert!(
        process_pending(
            true,
            true,
            || Ok(()),
            || Err(io::Error::other("save failed"))
        )
        .is_err()
    );
}
