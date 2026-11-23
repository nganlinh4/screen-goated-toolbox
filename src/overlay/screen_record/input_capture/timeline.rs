//! Monotonic timestamps for hook events, placed on the capture's own timeline.
//!
//! Hook procedures only read a monotonic clock. The capture backend publishes
//! the `Instant` its video, audio, and cursor timelines start from; events are
//! converted against that origin when drained, so keystrokes line up with the
//! picture and a wall-clock step cannot shift them.

use parking_lot::Mutex;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

static ANCHOR: LazyLock<Instant> = LazyLock::new(Instant::now);
static ORIGIN: Mutex<Option<Instant>> = Mutex::new(None);
static CAPTURE_START: Mutex<Option<Instant>> = Mutex::new(None);

/// Monotonic event stamp; cheap and lock-free for use inside hook procedures.
pub(super) fn event_stamp_ns() -> u64 {
    ANCHOR.elapsed().as_nanos() as u64
}

/// Fallback origin for a session whose backend never published one.
pub(super) fn mark_capture_start() {
    LazyLock::force(&ANCHOR);
    *CAPTURE_START.lock() = Some(Instant::now());
}

/// Publishes the instant the recording's media timeline starts at.
pub fn set_timeline_origin(origin: Instant) {
    LazyLock::force(&ANCHOR);
    *ORIGIN.lock() = Some(origin);
}

/// Ends the session's origin once its events have been handed out.
pub(super) fn clear_timeline_origin() {
    *ORIGIN.lock() = None;
}

pub(super) fn seconds_on_timeline(stamp_ns: u64) -> f64 {
    let at = *ANCHOR + Duration::from_nanos(stamp_ns);
    let origin = ORIGIN.lock().or(*CAPTURE_START.lock()).unwrap_or(*ANCHOR);
    at.saturating_duration_since(origin).as_secs_f64()
}

#[cfg(test)]
pub(super) fn anchor() -> Instant {
    *ANCHOR
}
