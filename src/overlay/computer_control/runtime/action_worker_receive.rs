//! Stop-aware receive loop for the action worker.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

const RECEIVE_POLL_INTERVAL: Duration = Duration::from_millis(50);

/// A session stop must wake an idle executor even while another sender clone
/// still exists. Channel disconnection remains a second, independent exit path.
pub(super) fn receive_until_stopped<T>(rx: &mpsc::Receiver<T>, stop: &AtomicBool) -> Option<T> {
    loop {
        if stop.load(Ordering::SeqCst) {
            return None;
        }
        match rx.recv_timeout(RECEIVE_POLL_INTERVAL) {
            Ok(value) => return Some(value),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return None,
        }
    }
}
