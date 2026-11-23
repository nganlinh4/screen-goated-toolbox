use std::sync::{
    Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

pub(crate) fn renderer_cancel_is_current(
    session: u64,
    generation: u64,
    current_session: u64,
    current_generation: u64,
    stopping: bool,
) -> bool {
    session != 0
        && generation != 0
        && session == current_session
        && generation == current_generation
        && !stopping
}

pub(super) fn commit_download_cancel(
    publication: &Mutex<()>,
    engine_revision: &AtomicU64,
    options_revision: &AtomicU64,
    model_changed: &AtomicBool,
    admit: impl FnOnce() -> bool,
    cancel: impl FnOnce(),
    select_model: impl FnOnce(),
) -> bool {
    let _publication = publication
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if !admit() {
        return false;
    }
    cancel();
    select_model();
    engine_revision.fetch_add(1, Ordering::SeqCst);
    options_revision.fetch_add(1, Ordering::SeqCst);
    model_changed.store(true, Ordering::SeqCst);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, mpsc};

    #[test]
    fn session_replacement_after_dispatch_cannot_cancel_the_replacement() {
        let publication = Mutex::new(());
        let session = AtomicU64::new(7);
        let engine = AtomicU64::new(0);
        let options = AtomicU64::new(0);
        let changed = AtomicBool::new(false);
        let cancelled = AtomicBool::new(false);
        let admitted_by_dispatch = session.load(Ordering::SeqCst) == 7;
        session.store(8, Ordering::SeqCst);
        if admitted_by_dispatch {
            commit_download_cancel(
                &publication,
                &engine,
                &options,
                &changed,
                || session.load(Ordering::SeqCst) == 7,
                || cancelled.store(true, Ordering::SeqCst),
                || {},
            );
        }
        assert!(
            !cancelled.load(Ordering::SeqCst),
            "replacement session was cancelled"
        );
        assert_eq!(engine.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn current_owner_commits_both_effects_and_revisions() {
        let publication = Mutex::new(());
        let engine = AtomicU64::new(3);
        let options = AtomicU64::new(5);
        let changed = AtomicBool::new(false);
        let cancelled = AtomicBool::new(false);
        let selected = AtomicBool::new(false);
        assert!(commit_download_cancel(
            &publication,
            &engine,
            &options,
            &changed,
            || true,
            || cancelled.store(true, Ordering::SeqCst),
            || selected.store(true, Ordering::SeqCst)
        ));
        assert!(cancelled.load(Ordering::SeqCst) && selected.load(Ordering::SeqCst));
        assert!(changed.load(Ordering::SeqCst));
        assert_eq!(engine.load(Ordering::SeqCst), 4);
        assert_eq!(options.load(Ordering::SeqCst), 6);
    }

    #[test]
    fn session_renderer_and_stop_state_all_gate_cancellation() {
        assert!(renderer_cancel_is_current(7, 3, 7, 3, false));
        assert!(!renderer_cancel_is_current(7, 3, 8, 3, false));
        assert!(!renderer_cancel_is_current(7, 3, 7, 4, false));
        assert!(!renderer_cancel_is_current(7, 3, 7, 3, true));
        assert!(!renderer_cancel_is_current(0, 3, 0, 3, false));
        assert!(!renderer_cancel_is_current(7, 0, 7, 0, false));
    }

    #[test]
    fn replacement_under_publication_lock_fences_waiting_cancel() {
        let publication = Arc::new(Mutex::new(()));
        let session = Arc::new(AtomicU64::new(7));
        let cancelled = Arc::new(AtomicBool::new(false));
        let held = publication.lock().unwrap();
        let (started_tx, started_rx) = mpsc::channel();
        let worker = {
            let publication = publication.clone();
            let session = session.clone();
            let cancelled = cancelled.clone();
            std::thread::spawn(move || {
                let engine = AtomicU64::new(0);
                let options = AtomicU64::new(0);
                let changed = AtomicBool::new(false);
                started_tx.send(()).unwrap();
                commit_download_cancel(
                    &publication,
                    &engine,
                    &options,
                    &changed,
                    || renderer_cancel_is_current(7, 3, session.load(Ordering::SeqCst), 3, false),
                    || cancelled.store(true, Ordering::SeqCst),
                    || {},
                )
            })
        };
        started_rx.recv().unwrap();
        session.store(8, Ordering::SeqCst);
        drop(held);
        assert!(!worker.join().unwrap());
        assert!(!cancelled.load(Ordering::SeqCst));
    }
}
