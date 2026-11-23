use super::save_current_with;
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

#[test]
fn snapshot_releases_state_during_write_and_preserves_writer_order() {
    let state = Arc::new(Mutex::new(("draft one", "old setting")));
    let gate = Arc::new(Mutex::new(()));
    let output = Arc::new(Mutex::new(Vec::new()));
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();

    std::thread::scope(|scope| {
        let first_state = Arc::clone(&state);
        let first_gate = Arc::clone(&gate);
        let first_output = Arc::clone(&output);
        let first = scope.spawn(move || {
            save_current_with(
                &first_state,
                &first_gate,
                |s| *s,
                |snapshot| {
                    entered_tx.send(()).unwrap();
                    release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                    first_output.lock().unwrap().push(*snapshot);
                    Ok(())
                },
            )
            .unwrap();
        });
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        // The background write is deliberately stalled; state remains available.
        {
            let mut current = state.try_lock().expect("state lock held during write");
            *current = ("draft two", "new setting");
        }
        let next_state = Arc::clone(&state);
        let next_gate = Arc::clone(&gate);
        let next_output = Arc::clone(&output);
        let second = scope.spawn(move || {
            let current = next_state.lock().unwrap();
            let _guard = next_gate.lock().unwrap();
            next_output.lock().unwrap().push(*current);
        });
        release_tx.send(()).unwrap();
        first.join().unwrap();
        second.join().unwrap();
    });
    assert_eq!(
        *output.lock().unwrap(),
        [("draft one", "old setting"), ("draft two", "new setting"),]
    );
}
