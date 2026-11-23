use std::sync::atomic::{AtomicU8, Ordering};

const IDLE: u8 = 0;
const DRAGGING: u8 = 1;
const CANCELLED: u8 = 2;

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Cancel {
    Cancelled,
    AlreadyCancelled,
    Idle,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Release {
    Dragging,
    Cancelled,
    Idle,
}

pub(super) struct Gesture(AtomicU8);

impl Gesture {
    pub const fn new() -> Self {
        Self(AtomicU8::new(IDLE))
    }

    pub fn reset(&self) {
        self.0.store(IDLE, Ordering::SeqCst);
    }

    pub fn start(&self) -> bool {
        self.0
            .compare_exchange(IDLE, DRAGGING, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    pub fn is_dragging(&self) -> bool {
        self.0.load(Ordering::SeqCst) == DRAGGING
    }

    pub fn cancel(&self) -> Cancel {
        match self
            .0
            .compare_exchange(DRAGGING, CANCELLED, Ordering::SeqCst, Ordering::SeqCst)
        {
            Ok(_) => Cancel::Cancelled,
            Err(CANCELLED) => Cancel::AlreadyCancelled,
            Err(_) => Cancel::Idle,
        }
    }

    pub fn release(&self) -> Release {
        match self.0.swap(IDLE, Ordering::SeqCst) {
            DRAGGING => Release::Dragging,
            CANCELLED => Release::Cancelled,
            _ => Release::Idle,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancellation_waits_for_the_matching_button_release() {
        let gesture = Gesture::new();
        assert!(gesture.start());
        assert_eq!(gesture.cancel(), Cancel::Cancelled);
        assert!(!gesture.is_dragging());
        assert_eq!(gesture.cancel(), Cancel::AlreadyCancelled);
        assert!(!gesture.start());
        assert_eq!(gesture.release(), Release::Cancelled);
        assert_eq!(gesture.release(), Release::Idle);
        assert!(gesture.start());
        assert_eq!(gesture.release(), Release::Dragging);
        assert_eq!(gesture.cancel(), Cancel::Idle);
    }
}
