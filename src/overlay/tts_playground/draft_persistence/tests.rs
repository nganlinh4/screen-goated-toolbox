use super::*;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;

#[test]
fn burst_and_close_flush_the_latest_draft_once() {
    let draft = Arc::new(Mutex::new(String::new()));
    let saved = Arc::new(Mutex::new(Vec::new()));
    let saves = Arc::clone(&saved);
    let snapshot = Arc::clone(&draft);
    let saver = DraftSaver::new(Duration::from_secs(60), move || {
        saves.lock().unwrap().push(snapshot.lock().unwrap().clone());
        Ok(())
    });
    for value in ["a", "ab", "abc"] {
        saver
            .update(|| {
                *draft.lock().unwrap() = value.to_string();
                Ok(true)
            })
            .unwrap();
    }
    saver.flush(false).unwrap();
    assert_eq!(*saved.lock().unwrap(), ["abc"]);
}

#[test]
fn edit_during_write_is_saved_after_the_in_flight_snapshot() {
    let draft = Arc::new(Mutex::new(String::new()));
    let saved = Arc::new(Mutex::new(Vec::new()));
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let release = Mutex::new(release_rx);
    let first = AtomicBool::new(true);
    let snapshot = Arc::clone(&draft);
    let saves = Arc::clone(&saved);
    let saver = DraftSaver::new(Duration::from_millis(10), move || {
        let value = snapshot.lock().unwrap().clone();
        if first.swap(false, Ordering::SeqCst) {
            started_tx.send(()).unwrap();
            release.lock().unwrap().recv().unwrap();
        }
        saves.lock().unwrap().push(value);
        Ok(())
    });
    saver
        .update(|| {
            *draft.lock().unwrap() = "first".into();
            Ok(true)
        })
        .unwrap();
    started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    saver
        .update(|| {
            *draft.lock().unwrap() = "second".into();
            Ok(true)
        })
        .unwrap();
    release_tx.send(()).unwrap();
    saver.flush(false).unwrap();
    assert_eq!(*saved.lock().unwrap(), ["first", "second"]);
}

#[test]
fn failed_close_can_retry_and_exit_freezes_new_edits() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&attempts);
    let saver = DraftSaver::new(Duration::from_secs(60), move || {
        if count.fetch_add(1, Ordering::SeqCst) == 0 {
            Err(io::Error::other("disk unavailable"))
        } else {
            Ok(())
        }
    });
    saver.update(|| Ok(true)).unwrap();
    assert!(saver.flush(true).is_err());
    saver.update(|| Ok(true)).unwrap();
    saver.flush(true).unwrap();
    assert!(saver.update(|| Ok(true)).is_err());
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
}

#[test]
fn aborted_exit_thaws_after_a_successful_flush() {
    let saver = DraftSaver::new(Duration::from_secs(60), || Ok(()));
    saver.update(|| Ok(true)).unwrap();
    saver.flush(true).unwrap();
    assert!(saver.update(|| Ok(true)).is_err());
    saver.thaw();
    assert!(saver.update(|| Ok(true)).is_ok());
    saver.flush(false).unwrap();
}
