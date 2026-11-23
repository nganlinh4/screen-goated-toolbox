use super::super::{MAX_RETAINED_TERMINAL_JOBS, idle_status};
use super::*;

#[test]
fn slow_admission_leaves_worker_state_available_and_defers_shutdown() {
    let name = format!(
        "{}::slow_admission_leaves_worker_state_available_and_defers_shutdown",
        module_path!().split_once("::").unwrap().1
    );
    crate::overlay::creation_submission_test::run_isolated(&name, |_| {
        let admission = acquire();
        let (done, wait) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            STATE
                .lock()
                .unwrap()
                .insert_job("progress".into(), idle_status());
            done.send(()).unwrap();
        });
        wait.recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        worker.join().unwrap();
        assert!(
            try_acquire().is_none(),
            "recovery must not read an unregistered intent"
        );
        assert!(!super::super::cancellation::cancel_for_shutdown());
        assert!(
            !super::super::super::shutdown(),
            "close must wait for admission registration"
        );
        drop(admission);
        assert!(try_acquire().is_some());
        assert!(super::super::super::shutdown());
    });
}

#[test]
fn reserved_revision_survives_expiry_and_retention_while_admission_performs_io() {
    let name = format!(
        "{}::reserved_revision_survives_expiry_and_retention_while_admission_performs_io",
        module_path!().split_once("::").unwrap().1
    );
    crate::overlay::creation_submission_test::run_isolated(&name, |_| {
        let mut admission = acquire();
        {
            let mut state = STATE.lock().unwrap();
            let mut continuation = super::super::tests::continuation();
            continuation.expires_at_ms = 10;
            let mut status = idle_status();
            status.stage = "done".into();
            state.insert_job("parent".into(), status.clone());
            state.continuations.insert("parent".into(), continuation);
            admission.reserve_continuation(&mut state, "parent");
            for index in 0..MAX_RETAINED_TERMINAL_JOBS + 1 {
                state.insert_job(format!("other-{index}"), status.clone());
            }
            state.prune_expired_continuations(11);
            assert!(state.continuations.contains_key("parent"));
            assert!(state.jobs.contains_key("parent"));
        }
        drop(admission);
        let mut state = STATE.lock().unwrap();
        assert!(state.reserved_continuations.is_empty());
        state.prune_expired_continuations(11);
        assert!(!state.continuations.contains_key("parent"));
        state.prune_terminal_jobs();
        assert!(!state.jobs.contains_key("parent"));
    });
}
