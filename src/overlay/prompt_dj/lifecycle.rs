use std::sync::Mutex;

use crate::win_types::SendHwnd;

#[derive(Default)]
pub(super) struct Lifecycle {
    pub hwnd: SendHwnd,
    pub active: bool,
    pub ready: bool,
    /// Whether this owner ever became ready; cleared when the owner finishes.
    pub launched: bool,
    pub closing: bool,
    pub show_generation: usize,
    pub acknowledged_show: usize,
    pub removing: bool,
}

pub(super) enum ShowAction {
    Start,
    Show(SendHwnd, usize),
    Wait,
}

impl Lifecycle {
    pub fn request_show(&mut self) -> ShowAction {
        if self.removing {
            return ShowAction::Wait;
        }
        self.show_generation = self.show_generation.wrapping_add(1);
        if self.ready && !self.closing && !self.hwnd.is_invalid() {
            return ShowAction::Show(self.hwnd, self.show_generation);
        }
        if self.active {
            ShowAction::Wait
        } else {
            self.active = true;
            ShowAction::Start
        }
    }

    pub fn mark_ready(&mut self) {
        self.ready = true;
        self.launched = true;
    }

    /// A launch the user asked for ended before the window could ever be shown.
    pub fn launch_failed(&self) -> bool {
        self.active && !self.launched && !self.removing
    }

    pub fn begin_close(&mut self) {
        if !self.closing {
            // A failed initial launch must not retry itself. A later explicit
            // show request during teardown still gets handed to a new owner.
            if !self.ready {
                self.acknowledged_show = self.show_generation;
            }
            self.closing = true;
            self.ready = false;
        }
    }

    pub fn can_show(&self, hwnd: SendHwnd) -> bool {
        !self.closing && !self.removing && self.ready && !hwnd.is_invalid() && self.hwnd.0 == hwnd.0
    }

    pub fn acknowledge_show(&mut self, hwnd: SendHwnd, generation: usize) {
        if self.can_show(hwnd) && generation == self.show_generation {
            self.acknowledged_show = generation;
        }
    }

    pub fn finish(&mut self) -> bool {
        let restart = self.closing && self.has_pending_show() && !self.removing;
        self.hwnd = SendHwnd::default();
        self.active = restart;
        self.ready = false;
        self.launched = false;
        self.closing = false;
        if !restart {
            self.acknowledged_show = self.show_generation;
        }
        restart
    }

    pub fn has_pending_show(&self) -> bool {
        self.show_generation != self.acknowledged_show
    }
}

pub(super) static STATE: Mutex<Lifecycle> = Mutex::new(Lifecycle {
    hwnd: SendHwnd(windows::Win32::Foundation::HWND(std::ptr::null_mut())),
    active: false,
    ready: false,
    launched: false,
    closing: false,
    show_generation: 0,
    acknowledged_show: 0,
    removing: false,
});
pub(super) static REMOVAL: Mutex<()> = Mutex::new(());

pub(super) struct RemovalWindow;

impl RemovalWindow {
    pub fn begin() -> Self {
        STATE
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .removing = true;
        Self
    }
}

impl Drop for RemovalWindow {
    fn drop(&mut self) {
        STATE
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .removing = false;
    }
}

/// Retires the current owner and reports a launch that never became ready.
/// Returns whether a show queued during teardown needs a new owner.
pub(super) fn finish_owner(on_failure: fn()) -> bool {
    let (restart, failed) = {
        let mut state = STATE.lock().unwrap_or_else(|error| error.into_inner());
        let failed = state.launch_failed();
        (state.finish(), failed)
    };
    if failed {
        on_failure();
    }
    restart
}

pub(super) struct ActiveWindow {
    pub on_failure: fn(),
}

impl Drop for ActiveWindow {
    fn drop(&mut self) {
        // The owning thread must release its window even if setup unwinds.
        let hwnd = STATE.lock().unwrap_or_else(|error| error.into_inner()).hwnd;
        if !hwnd.is_invalid() {
            unsafe {
                let _ = windows::Win32::UI::WindowsAndMessaging::DestroyWindow(hwnd.0);
            }
        }
        if finish_owner(self.on_failure) {
            super::start_window_thread();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_initialization_can_be_retried() {
        let mut state = Lifecycle::default();
        assert!(matches!(state.request_show(), ShowAction::Start));
        assert!(matches!(state.request_show(), ShowAction::Wait));
        state.begin_close();
        assert!(!state.finish());
        assert!(matches!(state.request_show(), ShowAction::Start));
    }

    #[test]
    fn queued_show_after_close_is_transferred_to_the_next_owner() {
        let mut state = Lifecycle::default();
        assert!(matches!(state.request_show(), ShowAction::Start));
        state.ready = true;
        state.hwnd = SendHwnd(windows::Win32::Foundation::HWND(std::ptr::dangling_mut()));
        let first = state.show_generation;
        assert!(matches!(state.request_show(), ShowAction::Show(_, _)));
        state.acknowledge_show(state.hwnd, first);
        state.begin_close();
        assert!(state.finish());
        assert!(state.active);
        assert!(state.has_pending_show());
    }

    #[test]
    fn show_requested_during_teardown_is_transferred_to_the_next_owner() {
        let mut state = Lifecycle::default();
        assert!(matches!(state.request_show(), ShowAction::Start));
        state.ready = true;
        state.hwnd = SendHwnd(windows::Win32::Foundation::HWND(std::ptr::dangling_mut()));
        state.acknowledge_show(state.hwnd, state.show_generation);
        state.begin_close();
        assert!(matches!(state.request_show(), ShowAction::Wait));
        assert!(state.finish());
        assert!(state.active);
        assert!(state.has_pending_show());
    }

    #[test]
    fn reentrant_close_before_show_acknowledgment_keeps_the_request() {
        let mut state = Lifecycle::default();
        assert!(matches!(state.request_show(), ShowAction::Start));
        state.ready = true;
        state.hwnd = SendHwnd(windows::Win32::Foundation::HWND(std::ptr::dangling_mut()));
        let hwnd = state.hwnd;
        let generation = state.show_generation;
        assert!(state.can_show(hwnd));
        state.begin_close();
        state.acknowledge_show(hwnd, generation);
        assert!(state.finish());
    }

    #[test]
    fn nested_show_acknowledgments_do_not_move_backward() {
        let mut state = Lifecycle::default();
        assert!(matches!(state.request_show(), ShowAction::Start));
        state.ready = true;
        state.hwnd = SendHwnd(windows::Win32::Foundation::HWND(std::ptr::dangling_mut()));
        let first = state.show_generation;
        assert!(matches!(state.request_show(), ShowAction::Show(_, _)));
        let second = state.show_generation;
        state.acknowledge_show(state.hwnd, second);
        state.acknowledge_show(state.hwnd, first);
        assert!(!state.has_pending_show());
        state.begin_close();
        assert!(!state.finish());
    }

    #[test]
    fn owner_guard_clears_and_reports_failed_initialization() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static REPORTED: AtomicUsize = AtomicUsize::new(0);
        fn report() {
            REPORTED.fetch_add(1, Ordering::SeqCst);
        }
        {
            let mut state = STATE.lock().unwrap();
            assert!(matches!(state.request_show(), ShowAction::Start));
        }
        {
            let _owner = ActiveWindow { on_failure: report };
        }
        assert_eq!(REPORTED.load(Ordering::SeqCst), 1);
        let mut state = STATE.lock().unwrap();
        assert!(matches!(state.request_show(), ShowAction::Start));
        assert!(!state.finish());
        assert_eq!(REPORTED.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn only_a_launch_that_never_became_ready_is_reported_as_failed() {
        let mut failed = Lifecycle::default();
        assert!(matches!(failed.request_show(), ShowAction::Start));
        failed.begin_close();
        assert!(failed.launch_failed());
        failed.finish();
        assert!(!failed.launch_failed());

        let mut opened = Lifecycle::default();
        assert!(matches!(opened.request_show(), ShowAction::Start));
        opened.mark_ready();
        opened.begin_close();
        assert!(!opened.launch_failed());
        // The unacknowledged show restarts a fresh owner that has not launched.
        assert!(opened.finish());
        assert!(opened.launch_failed());

        let mut removed = Lifecycle::default();
        assert!(matches!(removed.request_show(), ShowAction::Start));
        removed.removing = true;
        assert!(!removed.launch_failed());
    }

    #[test]
    fn removal_blocks_relaunch_until_it_finishes() {
        let mut state = Lifecycle::default();
        assert!(matches!(state.request_show(), ShowAction::Start));
        state.removing = true;
        state.finish();
        assert!(matches!(state.request_show(), ShowAction::Wait));
        assert!(!state.active);
        state.removing = false;
        assert!(matches!(state.request_show(), ShowAction::Start));
    }
}
