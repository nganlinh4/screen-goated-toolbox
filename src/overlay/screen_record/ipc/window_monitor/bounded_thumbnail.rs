//! Window thumbnails with a deadline and at most one capture per window.
//!
//! `PrintWindow` sends a synchronous `WM_PRINT` to the target app's UI thread.
//! The responsiveness probe skips windows that are already hung, but a window
//! can still hang after answering it, and then the capture thread blocks for
//! as long as the app does. The caller gives up after a deadline; a window
//! whose earlier capture is still blocked gets no new thread, so repeated
//! picker refreshes cannot pile up threads and window-sized bitmaps behind
//! one hung app.

use parking_lot::Mutex;
use std::collections::HashSet;
use std::sync::LazyLock;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use windows::Win32::Foundation::HWND;

const CAPTURE_DEADLINE: Duration = Duration::from_millis(500);

/// Windows whose capture thread has not returned yet.
static IN_FLIGHT: LazyLock<Mutex<HashSet<usize>>> = LazyLock::new(|| Mutex::new(HashSet::new()));

/// Removes its window from `IN_FLIGHT` when the capture returns or panics.
struct InFlight(usize);

impl Drop for InFlight {
    fn drop(&mut self) {
        IN_FLIGHT.lock().remove(&self.0);
    }
}

pub(in super::super) fn capture_window_thumbnail_with_timeout(hwnd: HWND) -> Option<String> {
    let hwnd_val = hwnd.0 as usize;
    run_bounded(hwnd_val, CAPTURE_DEADLINE, move || {
        crate::overlay::screen_record::capture_window_thumbnail(HWND(
            hwnd_val as *mut std::ffi::c_void,
        ))
    })
}

/// Runs `work` on its own thread and waits up to `deadline` for it. Returns
/// `None` without starting anything while an earlier run for `key` is still
/// going.
fn run_bounded<T, F>(key: usize, deadline: Duration, work: F) -> Option<T>
where
    T: Send + 'static,
    F: FnOnce() -> Option<T> + Send + 'static,
{
    if !IN_FLIGHT.lock().insert(key) {
        return None;
    }
    let (tx, rx) = mpsc::channel();
    let spawned = thread::Builder::new()
        .name("sr-window-thumbnail".into())
        .spawn(move || {
            let in_flight = InFlight(key);
            let result = work();
            // Release first, so a caller that just got this result can
            // capture the same window again straight away.
            drop(in_flight);
            let _ = tx.send(result);
        });
    if spawned.is_err() {
        IN_FLIGHT.lock().remove(&key);
        return None;
    }
    rx.recv_timeout(deadline).ok().flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Instant;

    #[test]
    fn a_window_whose_capture_is_still_blocked_gets_no_second_thread() {
        let key = 0x5eed_0001;
        let runs = Arc::new(AtomicUsize::new(0));
        let (release, blocked) = mpsc::channel::<()>();

        let counted = Arc::clone(&runs);
        let hung = run_bounded(key, Duration::from_millis(50), move || {
            counted.fetch_add(1, Ordering::SeqCst);
            blocked.recv().ok().map(|()| 1)
        });
        assert_eq!(hung, None, "the caller gives up at the deadline");

        let counted = Arc::clone(&runs);
        let retry = run_bounded(key, Duration::from_secs(1), move || {
            counted.fetch_add(1, Ordering::SeqCst);
            Some(2)
        });
        assert_eq!(retry, None);
        assert_eq!(
            runs.load(Ordering::SeqCst),
            1,
            "no thread for a blocked window"
        );

        let other = run_bounded(key + 1, Duration::from_secs(1), || Some(3));
        assert_eq!(other, Some(3), "other windows are unaffected");

        release.send(()).unwrap();
        let until = Instant::now() + Duration::from_secs(5);
        while IN_FLIGHT.lock().contains(&key) {
            assert!(
                Instant::now() < until,
                "the unblocked capture releases its window"
            );
            thread::sleep(Duration::from_millis(5));
        }
        let recovered = run_bounded(key, Duration::from_secs(1), || Some(4));
        assert_eq!(
            recovered,
            Some(4),
            "the window is captured again once it answers"
        );
    }
}
