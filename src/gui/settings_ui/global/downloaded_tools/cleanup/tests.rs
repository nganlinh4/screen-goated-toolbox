use super::{
    CLEANUP_STATE, CLEANUP_STEP_COUNT, CleanupDialogState, cleanup_fraction, request_confirmation,
    set_state,
};
use std::sync::Mutex;

static STATE_TEST_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn clean_all_first_opens_confirmation_without_starting_work() {
    let _guard = STATE_TEST_LOCK.lock().unwrap();
    set_state(CleanupDialogState::Idle);
    request_confirmation();
    assert!(matches!(
        *CLEANUP_STATE.lock().expect("cleanup state"),
        CleanupDialogState::Confirming
    ));
    set_state(CleanupDialogState::Idle);
}

#[test]
fn aborted_or_panicking_cleanup_releases_the_modal_with_attention() {
    let _guard = STATE_TEST_LOCK.lock().unwrap();
    let _cache = super::super::utils::CACHE_TEST_LOCK.lock().unwrap();
    let mutated = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let refreshed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let check = mutated.clone();
    let observed = refreshed.clone();
    set_state(CleanupDialogState::Running {
        completed: 0,
        step: super::CleanupStep::Interfaces,
    });
    let result = std::panic::catch_unwind(|| {
        let mut completion = super::completion::CleanupCompletion::new(
            eframe::egui::Context::default(),
            move || {
                observed.store(
                    check.load(std::sync::atomic::Ordering::SeqCst),
                    std::sync::atomic::Ordering::SeqCst,
                );
            },
        );
        completion.mark_started();
        mutated.store(true, std::sync::atomic::Ordering::SeqCst);
        panic!("injected cleanup failure");
    });
    assert!(result.is_err());
    assert!(refreshed.load(std::sync::atomic::Ordering::SeqCst));
    assert!(matches!(
        *CLEANUP_STATE.lock().unwrap(),
        CleanupDialogState::Complete { attention_count: 1 }
    ));
    set_state(CleanupDialogState::Running {
        completed: 0,
        step: super::CleanupStep::Interfaces,
    });
    let completion =
        super::completion::CleanupCompletion::new(eframe::egui::Context::default(), || {});
    let never_started = move || drop(completion);
    drop(never_started);
    assert!(matches!(
        *CLEANUP_STATE.lock().unwrap(),
        CleanupDialogState::Complete { attention_count: 1 }
    ));
    set_state(CleanupDialogState::Idle);
}

#[test]
fn cleanup_progress_is_bounded_and_finishes_at_one() {
    assert_eq!(cleanup_fraction(0), 0.0);
    assert_eq!(cleanup_fraction(CLEANUP_STEP_COUNT), 1.0);
    assert_eq!(cleanup_fraction(usize::MAX), 1.0);
}

#[test]
fn quiescence_outlives_refresh_even_when_cleanup_unwinds() {
    struct Lease(std::sync::Arc<std::sync::atomic::AtomicBool>);
    impl Drop for Lease {
        fn drop(&mut self) {
            self.0.store(false, std::sync::atomic::Ordering::SeqCst);
        }
    }
    let _guard = STATE_TEST_LOCK.lock().unwrap();
    let _cache = super::super::utils::CACHE_TEST_LOCK.lock().unwrap();
    let active = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let refresh_saw_lease = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let during_refresh = active.clone();
    let observed = refresh_saw_lease.clone();
    let result = std::panic::catch_unwind(|| {
        let mut completion = super::completion::CleanupCompletion::new(
            eframe::egui::Context::default(),
            move || {
                observed.store(
                    during_refresh.load(std::sync::atomic::Ordering::SeqCst),
                    std::sync::atomic::Ordering::SeqCst,
                );
            },
        );
        completion.mark_started();
        completion.hold_quiescence(Lease(active.clone()));
        panic!("injected cleanup failure");
    });
    assert!(result.is_err());
    assert!(refresh_saw_lease.load(std::sync::atomic::Ordering::SeqCst));
    assert!(!active.load(std::sync::atomic::Ordering::SeqCst));
    set_state(CleanupDialogState::Idle);
}
