//! Scoped activation for real keyboard and IME input inside the compositor WebView.

use std::sync::Mutex;

use windows::Win32::Foundation::HWND;
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, GWL_EXSTYLE, GetForegroundWindow, GetWindowLongPtrW,
    GetWindowThreadProcessId, IsWindow, SetForegroundWindow, SetWindowLongPtrW, WS_EX_NOACTIVATE,
};

use super::layout::CardRole;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Owner {
    generation: u64,
    session: u64,
    previous: isize,
}

#[derive(Default)]
struct FocusState {
    generation: u64,
    active: Option<Owner>,
    pending_cleanup: Option<Owner>,
}

impl FocusState {
    fn begin(&mut self, session: u64, previous: isize) -> Option<Owner> {
        let generation = self.generation.checked_add(1)?;
        let previous = if previous != 0 {
            previous
        } else {
            self.active
                .or(self.pending_cleanup)
                .map_or(0, |owner| owner.previous)
        };
        let owner = Owner {
            generation,
            session,
            previous,
        };
        self.generation = generation;
        self.active = Some(owner);
        self.pending_cleanup = None;
        Some(owner)
    }

    fn retire(&mut self, expected: Option<Owner>) -> Option<Owner> {
        let owner = if let Some(owner) = self.active {
            if expected.is_some_and(|expected| expected != owner) {
                return None;
            }
            owner
        } else if expected.is_none() {
            let previous = self.pending_cleanup?;
            self.generation = self.generation.checked_add(1)?;
            Owner {
                generation: self.generation,
                ..previous
            }
        } else {
            return None;
        };
        self.active = None;
        self.pending_cleanup = Some(owner);
        Some(owner)
    }

    fn owns(&self, owner: Owner) -> bool {
        self.active == Some(owner)
    }

    fn cleanup_is_current(&self, owner: Owner) -> bool {
        self.active.is_none()
            && self.generation == owner.generation
            && self.pending_cleanup == Some(owner)
    }

    fn cleanup_allowed(&self, owner: Owner, started_session: u64, current_session: u64) -> bool {
        self.cleanup_is_current(owner) && started_session == current_session
    }

    fn finish_cleanup(&mut self, owner: Owner) {
        if self.cleanup_is_current(owner) {
            self.pending_cleanup = None;
        }
    }
}

static FOCUS: Mutex<FocusState> = Mutex::new(FocusState {
    generation: 0,
    active: None,
    pending_cleanup: None,
});

fn lock_focus() -> std::sync::MutexGuard<'static, FocusState> {
    FOCUS.lock().unwrap_or_else(|error| error.into_inner())
}

fn current(owner: Owner) -> bool {
    lock_focus().owns(owner) && super::ipc::is_current_session(owner.session)
}

fn still_current(hwnd: HWND, owner: Owner) -> bool {
    if current(owner) {
        true
    } else {
        retire(hwnd, Some(owner));
        false
    }
}

fn may_restore_foreground(hwnd: isize, foreground: isize, previous: isize) -> bool {
    foreground == hwnd && previous != 0 && previous != hwnd
}

struct InputAttachment {
    current: u32,
    foreground: u32,
    attached: bool,
}

impl Drop for InputAttachment {
    fn drop(&mut self) {
        if self.attached {
            unsafe {
                let _ = AttachThreadInput(self.current, self.foreground, false);
            }
        }
    }
}

pub(super) fn begin(hwnd: HWND, role: CardRole, session_id: u64) {
    if !super::ipc::is_current_session(session_id) {
        return;
    }
    let foreground = unsafe { GetForegroundWindow() };
    let previous = if foreground == hwnd {
        0
    } else {
        foreground.0 as isize
    };
    let Some(owner) = lock_focus().begin(session_id, previous) else {
        return;
    };
    if !still_current(hwnd, owner) {
        return;
    }
    unsafe {
        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        if !still_current(hwnd, owner) {
            return;
        }
        let _ = SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style & !(WS_EX_NOACTIVATE.0 as isize));
        if !still_current(hwnd, owner) {
            return;
        }

        let foreground = GetForegroundWindow();
        let current_thread = GetCurrentThreadId();
        let foreground_thread = GetWindowThreadProcessId(foreground, None);
        let attachment = InputAttachment {
            current: current_thread,
            foreground: foreground_thread,
            attached: foreground_thread != 0
                && foreground_thread != current_thread
                && AttachThreadInput(current_thread, foreground_thread, true).as_bool(),
        };
        if !still_current(hwnd, owner) {
            return;
        }
        let _ = BringWindowToTop(hwnd);
        if !still_current(hwnd, owner) {
            return;
        }
        let _ = SetForegroundWindow(hwnd);
        drop(attachment);
    }
    if still_current(hwnd, owner) {
        super::webview::focus_card_text_input(role, session_id);
    }
}

pub(super) fn end(hwnd: HWND) {
    retire(hwnd, None);
}

fn retire(hwnd: HWND, expected: Option<Owner>) {
    let Some(owner) = lock_focus().retire(expected) else {
        return;
    };
    let cleanup_session = super::ipc::current_session();
    let cleanup_current =
        || lock_focus().cleanup_allowed(owner, cleanup_session, super::ipc::current_session());
    unsafe {
        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        if !cleanup_current() {
            return;
        }
        let _ = SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style | WS_EX_NOACTIVATE.0 as isize);
        if !cleanup_current() {
            return;
        }
        if owner.previous != 0 {
            let previous = HWND(owner.previous as *mut std::ffi::c_void);
            let foreground = GetForegroundWindow();
            if may_restore_foreground(hwnd.0 as isize, foreground.0 as isize, owner.previous)
                && IsWindow(Some(previous)).as_bool()
                && cleanup_current()
            {
                let _ = SetForegroundWindow(previous);
            }
        }
    }
    lock_focus().finish_cleanup(owner);
}

pub(super) fn is_active() -> bool {
    lock_focus().active.is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retired_begin_cannot_continue_or_retire_a_new_owner() {
        let mut state = FocusState::default();
        let old = state.begin(1, 42).unwrap();
        assert_eq!(state.retire(None), Some(old));
        let new = state.begin(2, 99).unwrap();
        assert_eq!(new.previous, 99);
        assert!(!state.owns(old));
        assert_eq!(state.retire(Some(old)), None);
        assert_eq!(state.active, Some(new));
        assert!(!state.cleanup_is_current(old));
    }

    #[test]
    fn nested_replacement_preserves_original_foreground_and_fences_old_cleanup() {
        let mut state = FocusState::default();
        let old = state.begin(1, 42).unwrap();
        let new = state.begin(2, 0).unwrap();
        assert_eq!(new.previous, 42);
        assert_eq!(state.retire(Some(old)), None);
        assert_eq!(state.retire(Some(new)), Some(new));
        assert!(state.cleanup_is_current(new));
        let replacement = state.begin(3, 77).unwrap();
        assert!(!state.cleanup_is_current(new));
        assert!(state.owns(replacement));
    }

    #[test]
    fn newer_session_can_take_over_pending_cleanup() {
        let mut state = FocusState::default();
        let old = state.begin(1, 42).unwrap();
        assert_eq!(state.retire(None), Some(old));
        assert!(state.cleanup_allowed(old, 1, 1));
        assert!(!state.cleanup_allowed(old, 1, 2));
        let takeover = state.retire(None).unwrap();
        assert_ne!(takeover.generation, old.generation);
        assert_eq!(takeover.previous, 42);
        assert!(!state.cleanup_is_current(old));
        assert!(state.cleanup_is_current(takeover));
        state.finish_cleanup(takeover);
        assert!(state.retire(None).is_none());
    }

    #[test]
    fn external_foreground_supersedes_an_older_pending_restore() {
        let mut state = FocusState::default();
        let old = state.begin(1, 42).unwrap();
        assert_eq!(state.retire(None), Some(old));
        let new = state.begin(2, 77).unwrap();
        assert_eq!(new.previous, 77);
        assert!(!state.cleanup_is_current(old));
    }

    #[test]
    fn cleanup_does_not_steal_a_new_external_foreground() {
        assert!(may_restore_foreground(10, 10, 42));
        assert!(!may_restore_foreground(10, 77, 42));
        assert!(!may_restore_foreground(10, 10, 0));
        assert!(!may_restore_foreground(10, 10, 10));
    }
}
