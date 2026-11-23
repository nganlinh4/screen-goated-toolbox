use super::*;
use std::sync::atomic::AtomicU64;
use std::sync::mpsc;
use std::time::Duration;

#[test]
fn capture_meter_skips_contended_and_stale_publications() {
    let publication = Mutex::new(());
    let session = AtomicU64::new(2);
    let revision = AtomicU64::new(8);
    let stopped = AtomicBool::new(false);
    let guard = publication.lock().unwrap();
    assert!(!try_publish_if_engine_current_with(
        &publication,
        &session,
        &revision,
        2,
        &stopped,
        8,
        || panic!("contended meter published")
    ));
    drop(guard);
    assert!(!try_publish_if_engine_current_with(
        &publication,
        &session,
        &revision,
        2,
        &stopped,
        7,
        || panic!("stale meter published")
    ));
    let mut published = false;
    assert!(try_publish_if_engine_current_with(
        &publication,
        &session,
        &revision,
        2,
        &stopped,
        8,
        || published = true
    ));
    assert!(published);
}

#[test]
fn delayed_engine_publication_is_rejected_after_method_or_source_change() {
    for _change in ["method", "source"] {
        let publication = Arc::new(Mutex::new(()));
        let current = Arc::new(AtomicU64::new(1));
        let revision = Arc::new(AtomicU64::new(4));
        let stopped = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let lock = publication.clone();
        let id = current.clone();
        let generation = revision.clone();
        let stop = stopped.clone();
        let worker = std::thread::spawn(move || {
            ready_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            publish_if_engine_current_with(&lock, &id, &generation, 1, &stop, 4, || {
                panic!("old engine published after its owner changed")
            })
        });
        ready_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        {
            let _guard = publication.lock().unwrap();
            revision.store(5, Ordering::SeqCst);
        }
        release_tx.send(()).unwrap();
        assert!(!worker.join().unwrap());
        assert!(publish_if_engine_current_with(
            &publication,
            &current,
            &revision,
            1,
            &stopped,
            5,
            || {}
        ));
    }
}

#[test]
fn session_transition_serializes_against_publication_and_rejects_late_events() {
    let publication = Arc::new(Mutex::new(()));
    let current = Arc::new(AtomicU64::new(1));
    let stopped = Arc::new(AtomicBool::new(false));
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let (transition_tx, transition_rx) = mpsc::channel();
    let (transition_started_tx, transition_started_rx) = mpsc::channel();
    let publish_lock = publication.clone();
    let publish_id = current.clone();
    let publish_stop = stopped.clone();
    let producer = std::thread::spawn(move || {
        publish_if_session_with(&publish_lock, &publish_id, 1, Some(&publish_stop), || {
            entered_tx.send(()).unwrap();
            release_rx.recv().unwrap();
        })
    });
    entered_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    let transition_lock = publication.clone();
    let transition_id = current.clone();
    let transition = std::thread::spawn(move || {
        transition_started_tx.send(()).unwrap();
        let _guard = transition_lock.lock().unwrap();
        transition_id.store(2, Ordering::SeqCst);
        transition_tx.send(()).unwrap();
    });
    transition_started_rx
        .recv_timeout(Duration::from_secs(1))
        .unwrap();
    assert!(
        transition_rx
            .recv_timeout(Duration::from_millis(50))
            .is_err()
    );
    release_tx.send(()).unwrap();
    assert!(producer.join().unwrap());
    transition.join().unwrap();
    assert!(!publish_if_session_with(
        &publication,
        &current,
        1,
        Some(&stopped),
        || panic!("stale publication")
    ));
    assert!(publish_if_session_with(
        &publication,
        &current,
        2,
        Some(&stopped),
        || {}
    ));
}

#[test]
fn replacing_session_cancels_previous_signal_without_stopping_new_one() {
    let sessions = SessionCancellation::new();
    let old = sessions.current();
    let new = sessions.begin_session();
    assert!(old.load(Ordering::SeqCst));
    assert!(!new.load(Ordering::SeqCst));
}

#[test]
fn restart_can_clear_same_session_stop_but_not_a_replaced_session_stop() {
    let publication = Mutex::new(());
    let current = AtomicU64::new(1);
    let stopped = AtomicBool::new(true);
    assert!(!publish_if_session_with(
        &publication,
        &current,
        1,
        Some(&stopped),
        || panic!("stopped session started")
    ));
    assert!(publish_if_session_with(
        &publication,
        &current,
        1,
        None,
        || {
            stopped.store(false, Ordering::SeqCst);
        }
    ));
    stopped.store(true, Ordering::SeqCst);
    current.store(2, Ordering::SeqCst);
    assert!(!publish_if_session_with(
        &publication,
        &current,
        1,
        None,
        || {
            stopped.store(false, Ordering::SeqCst);
        }
    ));
    assert!(stopped.load(Ordering::SeqCst));
}
