//! Serialize a close with show requests directed at the window it retires.

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CloseAttempt {
    pub(super) hwnd: isize,
    pub(super) id: usize,
}

#[derive(Default)]
struct CloseShow {
    closing: Option<CloseAttempt>,
    pending_show: bool,
    shutdown_owner: Option<u64>,
}

static STATE: Mutex<CloseShow> = Mutex::new(CloseShow {
    closing: None,
    pending_show: false,
    shutdown_owner: None,
});
static NEXT_ATTEMPT: AtomicUsize = AtomicUsize::new(1);

pub(super) fn begin_close(hwnd: isize) -> Option<CloseAttempt> {
    let mut state = STATE.lock().unwrap_or_else(|error| error.into_inner());
    if state.closing.is_some() {
        return None;
    }
    let id = loop {
        let id = NEXT_ATTEMPT.fetch_add(1, Ordering::Relaxed);
        if id != 0 {
            break id;
        }
    };
    let attempt = CloseAttempt { hwnd, id };
    state.closing = Some(attempt);
    Some(attempt)
}

/// Post while holding the state lock so an accepted close cannot overtake the post.
pub(super) fn show_or_defer(post: impl FnOnce()) {
    let mut state = STATE.lock().unwrap_or_else(|error| error.into_inner());
    if state.closing.is_some() {
        state.pending_show = true;
    } else {
        post();
    }
}

pub(super) fn defer_show_if_closing() -> bool {
    let mut state = STATE.lock().unwrap_or_else(|error| error.into_inner());
    if state.closing.is_none() {
        return false;
    }
    state.pending_show = true;
    true
}

pub(super) fn is_closing() -> bool {
    STATE
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .closing
        .is_some()
}

pub(super) fn mark_prepared(attempt: CloseAttempt, owner: u64) -> bool {
    let mut state = STATE.lock().unwrap_or_else(|error| error.into_inner());
    if state.closing != Some(attempt) {
        return false;
    }
    state.shutdown_owner = Some(owner);
    true
}

/// A stale window must not release a newer window's close or show request.
pub(super) fn finish_close(attempt: CloseAttempt) -> Option<(bool, Option<u64>)> {
    let mut state = STATE.lock().unwrap_or_else(|error| error.into_inner());
    if state.closing != Some(attempt) {
        return None;
    }
    state.closing = None;
    Some((
        std::mem::take(&mut state.pending_show),
        state.shutdown_owner.take(),
    ))
}

pub(super) fn prepared_owner(attempt: CloseAttempt) -> Option<u64> {
    let state = STATE.lock().unwrap_or_else(|error| error.into_inner());
    (state.closing == Some(attempt))
        .then_some(state.shutdown_owner)
        .flatten()
}

pub(super) fn finish_window(attempt: Option<CloseAttempt>) {
    if let Some(attempt) = attempt
        && let Some((show, owner)) = finish_close(attempt)
    {
        if let Some(owner) = owner {
            super::shutdown::release_closed_window_owner(owner);
        }
        if show {
            super::show_screen_record();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn show_after_close_waits_for_the_matching_window_to_finish() {
        let _guard = TEST_LOCK.lock().unwrap();
        let first = begin_close(101).unwrap();
        let mut posted = false;
        show_or_defer(|| posted = true);
        assert!(!posted);
        assert_eq!(
            finish_close(CloseAttempt {
                hwnd: 202,
                id: first.id
            }),
            None
        );
        assert!(is_closing());
        assert!(mark_prepared(first, 42));
        assert_eq!(finish_close(first), Some((true, Some(42))));
        assert!(!is_closing());
        show_or_defer(|| posted = true);
        assert!(posted);
        let second = begin_close(202).unwrap();
        assert_eq!(finish_close(first), None);
        assert_eq!(finish_close(second), Some((false, None)));
    }

    #[test]
    fn reused_hwnd_cannot_accept_an_older_close_completion() {
        let _guard = TEST_LOCK.lock().unwrap();
        let first = begin_close(313).unwrap();
        assert_eq!(finish_close(first), Some((false, None)));
        let second = begin_close(313).unwrap();
        assert_ne!(first.id, second.id);
        assert!(!mark_prepared(first, 12));
        assert_eq!(finish_close(first), None);
        assert_eq!(prepared_owner(first), None);
        show_or_defer(|| panic!("show must wait for the newer close"));
        assert!(mark_prepared(second, 13));
        assert_eq!(prepared_owner(first), None);
        assert_eq!(prepared_owner(second), Some(13));
        assert_eq!(finish_close(first), None);
        assert_eq!(finish_close(second), Some((true, Some(13))));
    }
}
