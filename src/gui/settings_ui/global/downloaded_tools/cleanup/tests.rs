use super::{
    CLEANUP_STATE, CLEANUP_STEP_COUNT, CleanupDialogState, cleanup_fraction, request_confirmation,
    set_state,
};

#[test]
fn clean_all_first_opens_confirmation_without_starting_work() {
    set_state(CleanupDialogState::Idle);
    request_confirmation();
    assert!(matches!(
        *CLEANUP_STATE.lock().expect("cleanup state"),
        CleanupDialogState::Confirming
    ));
    set_state(CleanupDialogState::Idle);
}

#[test]
fn cleanup_progress_is_bounded_and_finishes_at_one() {
    assert_eq!(cleanup_fraction(0), 0.0);
    assert_eq!(cleanup_fraction(CLEANUP_STEP_COUNT), 1.0);
    assert_eq!(cleanup_fraction(usize::MAX), 1.0);
}
