//! Geometry-only use of the shared drawing selector; cancellation returns no region.
use std::cell::Cell;
use windows::Win32::Foundation::RECT;

thread_local! {
    static DRAW: Cell<Option<Option<RECT>>> = const { Cell::new(None) };
}

pub(super) fn begin() {
    DRAW.set(Some(None));
}
pub(super) fn finish() -> Option<RECT> {
    DRAW.replace(None).flatten()
}
pub(super) fn active() -> bool {
    DRAW.get().is_some()
}
pub(super) fn commit(rect: RECT) -> bool {
    if !active() {
        return false;
    }
    DRAW.set(Some(Some(rect)));
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancellation_and_completion_leave_no_selector_state() {
        begin();
        assert_eq!(finish(), None);
        assert!(!active());
        let rect = RECT {
            left: -500,
            top: 50,
            right: -200,
            bottom: 150,
        };
        assert!(!commit(rect));
        begin();
        assert!(commit(rect));
        assert_eq!(finish(), Some(rect));
        assert!(!active());
    }
}
