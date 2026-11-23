//! Ownership of a single-instance mini-app window shared between the caller
//! threads that open it and the window thread that runs it.
//!
//! Every created window holds a generation. Teardown only clears the slot when
//! its own generation still owns it, so a closing window can never erase the
//! state of a replacement that was opened while it was shutting down.

use std::sync::Mutex;
use windows::Win32::Foundation::HWND;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum Phase {
    #[default]
    Closed,
    Initializing,
    Ready,
}

#[derive(Clone, Copy, Default)]
struct SlotState {
    generation: u64,
    phase: Phase,
    hwnd: isize,
}

pub(crate) struct WindowSlot {
    state: Mutex<SlotState>,
}

impl WindowSlot {
    pub(crate) const fn new() -> Self {
        Self {
            state: Mutex::new(SlotState {
                generation: 0,
                phase: Phase::Closed,
                hwnd: 0,
            }),
        }
    }

    fn with<T>(&self, effect: impl FnOnce(&mut SlotState) -> T) -> T {
        effect(&mut self.state.lock().unwrap_or_else(|error| error.into_inner()))
    }

    /// Atomically claims a closed slot for a new window and returns its
    /// generation. Returns `None` while a window is initializing or ready.
    pub(crate) fn try_claim(&self) -> Option<u64> {
        self.with(|state| {
            if state.phase != Phase::Closed {
                return None;
            }
            state.generation = state.generation.wrapping_add(1);
            state.phase = Phase::Initializing;
            state.hwnd = 0;
            Some(state.generation)
        })
    }

    /// Records the created window for `generation`; false when superseded.
    pub(crate) fn publish_hwnd(&self, generation: u64, hwnd: HWND) -> bool {
        self.with(|state| {
            let owns = state.generation == generation && state.phase == Phase::Initializing;
            if owns {
                state.hwnd = hwnd.0 as isize;
            }
            owns
        })
    }

    /// Marks `generation` ready for show requests; false when superseded.
    pub(crate) fn mark_ready(&self, generation: u64) -> bool {
        self.with(|state| {
            let owns = state.generation == generation && state.phase == Phase::Initializing;
            if owns {
                state.phase = Phase::Ready;
            }
            owns
        })
    }

    /// The window handle while a window is ready.
    pub(crate) fn ready_hwnd(&self) -> Option<HWND> {
        self.with(|state| {
            (state.phase == Phase::Ready && state.hwnd != 0)
                .then_some(HWND(state.hwnd as *mut std::ffi::c_void))
        })
    }

    /// The window handle while a window is initializing or ready.
    pub(crate) fn hwnd(&self) -> Option<HWND> {
        self.with(|state| {
            (state.phase != Phase::Closed && state.hwnd != 0)
                .then_some(HWND(state.hwnd as *mut std::ffi::c_void))
        })
    }

    #[cfg(test)]
    pub(crate) fn is_ready(&self) -> bool {
        self.with(|state| state.phase == Phase::Ready)
    }

    pub(crate) fn is_initializing(&self) -> bool {
        self.with(|state| state.phase == Phase::Initializing)
    }

    /// Whether `hwnd` is the ready window that owns the slot.
    pub(crate) fn is_ready_window(&self, hwnd: HWND) -> bool {
        self.ready_hwnd() == Some(hwnd)
    }

    /// Closes the slot when the destroyed `hwnd` still owns it.
    pub(crate) fn release_window(&self, hwnd: HWND) -> bool {
        self.with(|state| {
            let owns = state.phase != Phase::Closed && state.hwnd == hwnd.0 as isize;
            if owns {
                *state = SlotState {
                    generation: state.generation,
                    ..SlotState::default()
                };
            }
            owns
        })
    }

    /// Closes the slot when `generation` still owns it.
    pub(crate) fn release(&self, generation: u64) -> bool {
        self.with(|state| {
            let owns = state.generation == generation && state.phase != Phase::Closed;
            if owns {
                *state = SlotState {
                    generation,
                    ..SlotState::default()
                };
            }
            owns
        })
    }
}

#[cfg(test)]
mod tests {
    use super::WindowSlot;
    use windows::Win32::Foundation::HWND;

    fn hwnd(value: isize) -> HWND {
        HWND(value as *mut std::ffi::c_void)
    }

    #[test]
    fn only_one_caller_claims_a_closed_slot() {
        let slot = WindowSlot::new();
        let first = slot.try_claim();
        assert!(first.is_some());
        assert!(slot.try_claim().is_none());
        assert!(slot.is_initializing());
        assert!(slot.ready_hwnd().is_none());
    }

    #[test]
    fn concurrent_claims_create_exactly_one_window() {
        let slot = std::sync::Arc::new(WindowSlot::new());
        let claims = (0..8)
            .map(|_| {
                let slot = slot.clone();
                std::thread::spawn(move || slot.try_claim().is_some())
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|claim| claim.join().unwrap())
            .filter(|claimed| *claimed)
            .count();
        assert_eq!(claims, 1);
    }

    #[test]
    fn old_teardown_cannot_clobber_a_relaunched_window() {
        let slot = WindowSlot::new();
        let old = slot.try_claim().unwrap();
        assert!(slot.publish_hwnd(old, hwnd(10)));
        assert!(slot.mark_ready(old));
        assert!(slot.is_ready_window(hwnd(10)));

        // WM_DESTROY of the old window frees the slot for a relaunch.
        assert!(slot.release_window(hwnd(10)));
        let new = slot.try_claim().unwrap();
        assert!(slot.publish_hwnd(new, hwnd(20)));
        assert!(slot.mark_ready(new));

        // The old window thread finishes its teardown afterwards.
        assert!(!slot.release(old));
        assert!(!slot.release_window(hwnd(10)));
        assert!(!slot.publish_hwnd(old, hwnd(10)));
        assert!(!slot.mark_ready(old));
        assert_eq!(slot.ready_hwnd(), Some(hwnd(20)));
        assert!(slot.is_ready());

        assert!(slot.release(new));
        assert!(slot.ready_hwnd().is_none());
        assert!(slot.try_claim().is_some());
    }

    #[test]
    fn failed_creation_releases_its_claim() {
        let slot = WindowSlot::new();
        let generation = slot.try_claim().unwrap();
        assert!(slot.hwnd().is_none());
        assert!(slot.release(generation));
        assert!(!slot.is_initializing());
        assert!(slot.try_claim().is_some());
    }
}
