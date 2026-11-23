//! Serialize a command's terminal handoff with job cancellation requests.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

static HANDOFF: Mutex<()> = Mutex::new(());

pub(super) fn request_cancel(cancel: &AtomicBool) {
    let _guard = HANDOFF
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    cancel.store(true, Ordering::SeqCst);
}

pub(super) fn complete_unless_cancelled<T>(
    cancel: &AtomicBool,
    finish: impl FnOnce() -> T,
) -> Option<T> {
    admit_unless_cancelled(Some(cancel), finish)
}

pub(super) fn admit_unless_cancelled<T>(
    cancel: Option<&AtomicBool>,
    finish: impl FnOnce() -> T,
) -> Option<T> {
    let _guard = HANDOFF
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if cancel.is_some_and(|cancel| cancel.load(Ordering::SeqCst)) {
        None
    } else {
        Some(finish())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn cancellation_before_handoff_prevents_release() {
        let cancel = AtomicBool::new(false);
        request_cancel(&cancel);
        assert_eq!(
            complete_unless_cancelled(&cancel, || panic!("released after cancellation")),
            None::<()>
        );
    }

    #[test]
    fn cancellation_waits_for_an_in_progress_terminal_handoff() {
        let cancel = AtomicBool::new(false);
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (attempting_tx, attempting_rx) = mpsc::channel();
        let (requested_tx, requested_rx) = mpsc::channel();
        std::thread::scope(|scope| {
            let token = &cancel;
            let finish = scope.spawn(move || {
                complete_unless_cancelled(token, || {
                    entered_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                    7
                })
            });
            entered_rx.recv().unwrap();
            let request = scope.spawn(|| {
                attempting_tx.send(()).unwrap();
                request_cancel(&cancel);
                requested_tx.send(()).unwrap();
            });
            attempting_rx.recv().unwrap();
            let blocked = requested_rx
                .recv_timeout(Duration::from_millis(50))
                .is_err();
            let unset_during_handoff = !cancel.load(Ordering::SeqCst);
            release_tx.send(()).unwrap();
            assert_eq!(finish.join().unwrap(), Some(7));
            request.join().unwrap();
            assert!(blocked);
            assert!(unset_during_handoff);
            assert!(cancel.load(Ordering::SeqCst));
        });
    }
}
