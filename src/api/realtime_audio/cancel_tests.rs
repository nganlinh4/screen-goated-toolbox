use super::commit_download_cancel;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc,
};
use std::time::Duration;

#[test]
fn cancel_and_restart_selection_are_one_publication_transaction() {
    let publication = Arc::new(Mutex::new(()));
    let stopped = Arc::new(AtomicBool::new(false));
    let changed = Arc::new(AtomicBool::new(false));
    let engine = Arc::new(AtomicU64::new(0));
    let options = Arc::new(AtomicU64::new(0));
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let lock = publication.clone();
    let worker_stop = stopped.clone();
    let worker_changed = changed.clone();
    let worker_engine = engine.clone();
    let worker_options = options.clone();
    let worker = std::thread::spawn(move || {
        commit_download_cancel(
            &lock,
            &worker_engine,
            &worker_options,
            &worker_changed,
            || true,
            || {
                worker_stop.store(true, Ordering::SeqCst);
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
            },
            || {},
        );
    });
    entered_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    let (observed_tx, observed_rx) = mpsc::channel();
    let observer_lock = publication.clone();
    let observer_stop = stopped.clone();
    let observer_changed = changed.clone();
    let observer = std::thread::spawn(move || {
        let _guard = observer_lock.lock().unwrap();
        observed_tx
            .send((
                observer_stop.load(Ordering::SeqCst),
                observer_changed.load(Ordering::SeqCst),
            ))
            .unwrap();
    });
    assert!(observed_rx.recv_timeout(Duration::from_millis(50)).is_err());
    release_tx.send(()).unwrap();
    assert_eq!(
        observed_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
        (true, true)
    );
    worker.join().unwrap();
    observer.join().unwrap();
    assert_eq!(engine.load(Ordering::SeqCst), 1);
    assert_eq!(options.load(Ordering::SeqCst), 1);
}

#[test]
fn shared_cancellation_ownership_cases() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../parity-fixtures/live-translate/download-cancellation-ownership.json"
    ))
    .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let event = case["event"].as_str().unwrap();
        let (session, generation, stopping) = match event {
            "session_changes_after_dispatch" => (8, 3, false),
            "renderer_changes_after_dispatch" => (7, 4, false),
            "session_stopping" => (7, 3, true),
            "current_renderer_and_session" => (7, 3, false),
            _ => panic!("unknown cancellation ownership case"),
        };
        let current = super::renderer_cancel_is_current(7, 3, session, generation, stopping);
        let publication = Mutex::new(());
        let engine = AtomicU64::new(0);
        let options = AtomicU64::new(0);
        let changed = AtomicBool::new(false);
        let cancelled = AtomicBool::new(false);
        let selected = AtomicBool::new(false);
        commit_download_cancel(
            &publication,
            &engine,
            &options,
            &changed,
            || current,
            || cancelled.store(true, Ordering::SeqCst),
            || selected.store(true, Ordering::SeqCst),
        );
        let outcome = if cancelled.load(Ordering::SeqCst) && selected.load(Ordering::SeqCst) {
            "cancel_and_restore"
        } else {
            "none"
        };
        assert_eq!(outcome, case["effect"].as_str().unwrap(), "{event}");
    }
}
