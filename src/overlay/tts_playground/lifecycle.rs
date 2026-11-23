//! Serialized ownership of the TTS Playground window and its removal.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Phase {
    Idle,
    Starting,
    Ready,
    Stopping,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ShowAction {
    Start,
    Post(isize, usize),
    Deferred,
}

pub(super) struct WindowLifecycle {
    phase: Phase,
    hwnd: Option<isize>,
    show_pending: bool,
    removing: bool,
    exiting: bool,
    generation: usize,
    document_started: bool,
}

impl WindowLifecycle {
    pub(super) const fn new() -> Self {
        Self {
            phase: Phase::Idle,
            hwnd: None,
            show_pending: false,
            removing: false,
            exiting: false,
            generation: 0,
            document_started: false,
        }
    }

    pub(super) fn show(&mut self) -> ShowAction {
        if self.removing || self.exiting {
            return ShowAction::Deferred;
        }
        match self.phase {
            Phase::Idle => {
                let Some(generation) = self.generation.checked_add(1) else {
                    return ShowAction::Deferred;
                };
                self.generation = generation;
                self.phase = Phase::Starting;
                self.show_pending = true;
                ShowAction::Start
            }
            Phase::Starting | Phase::Stopping => {
                self.show_pending = true;
                ShowAction::Deferred
            }
            Phase::Ready => self.hwnd.map_or(ShowAction::Deferred, |hwnd| {
                ShowAction::Post(hwnd, self.generation)
            }),
        }
    }

    pub(super) fn created(&mut self, hwnd: isize) -> usize {
        self.hwnd = Some(hwnd);
        self.document_started = false;
        self.generation
    }

    /// A TTS window hosts exactly one local document. A later navigation,
    /// including reload, would invalidate an acknowledged edit barrier.
    pub(super) fn admit_initial_document(
        &mut self,
        hwnd: isize,
        generation: usize,
        expected: &str,
        actual: &str,
    ) -> bool {
        if expected != actual
            || self.hwnd != Some(hwnd)
            || self.generation != generation
            || self.document_started
            || !matches!(self.phase, Phase::Starting | Phase::Ready)
        {
            return false;
        }
        self.document_started = true;
        true
    }

    pub(super) fn ready(&mut self, hwnd: isize) -> Option<(usize, bool)> {
        if self.hwnd != Some(hwnd) || self.phase != Phase::Starting {
            return None;
        }
        self.phase = Phase::Ready;
        let show = self.show_pending && !self.removing;
        self.show_pending = false;
        Some((self.generation, show))
    }

    pub(super) fn ready_window(&self) -> Option<(isize, usize)> {
        (self.phase == Phase::Ready && !self.removing)
            .then_some(self.hwnd)
            .flatten()
            .map(|hwnd| (hwnd, self.generation))
    }

    pub(super) fn is_ready_window(&self, hwnd: isize, generation: usize) -> bool {
        self.phase == Phase::Ready && self.hwnd == Some(hwnd) && self.generation == generation
    }

    pub(super) fn is_ready_document(
        &self,
        hwnd: isize,
        generation: usize,
        document: usize,
    ) -> bool {
        document == 1 && self.document_started && self.is_ready_window(hwnd, generation)
    }

    pub(super) fn destroyed(&mut self, hwnd: isize) {
        if self.hwnd == Some(hwnd) {
            self.hwnd = None;
            self.phase = Phase::Stopping;
        }
    }

    pub(super) fn creation_failed(&mut self) {
        self.show_pending = false;
    }

    /// Called after the owning message thread has released its WebView.
    /// Returns true if a show requested during teardown should start afresh.
    pub(super) fn thread_finished(&mut self) -> bool {
        if self.phase == Phase::Ready && self.hwnd.is_some() {
            // If native destruction failed, do not publish an idle window or
            // permit removal based on an unproven teardown.
            self.phase = Phase::Stopping;
            self.show_pending = false;
            return false;
        }
        let restart = self.phase == Phase::Stopping
            && self.show_pending
            && !self.removing
            && !self.exiting
            && self.generation < usize::MAX;
        if restart {
            self.generation += 1;
        }
        self.phase = if restart {
            Phase::Starting
        } else {
            Phase::Idle
        };
        self.hwnd = None;
        self.show_pending = restart;
        restart
    }

    pub(super) fn begin_removal(&mut self) {
        self.removing = true;
        self.show_pending = false;
    }

    pub(super) fn end_removal(&mut self) {
        self.removing = false;
    }

    pub(super) fn begin_exit(&mut self) -> bool {
        if self.removing || self.exiting {
            return false;
        }
        self.exiting = true;
        true
    }

    pub(super) fn end_exit(&mut self) {
        self.exiting = false;
    }

    pub(super) fn exit_observation(&self) -> Result<Option<(isize, usize, usize)>, ()> {
        match self.phase {
            Phase::Idle => Ok(None),
            Phase::Ready if !self.removing && self.document_started => self
                .hwnd
                .map(|hwnd| Some((hwnd, self.generation, 1)))
                .ok_or(()),
            _ => Err(()),
        }
    }

    pub(super) fn removal_observation(&self) -> (Option<isize>, bool) {
        (
            self.hwnd,
            matches!(self.phase, Phase::Starting | Phase::Stopping),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removal_prevents_reopen_until_guard_is_released() {
        let mut state = WindowLifecycle::new();
        assert_eq!(state.show(), ShowAction::Start);
        state.created(42);
        assert_eq!(state.ready(42), Some((1, true)));
        state.begin_removal();
        assert_eq!(state.ready_window(), None);
        assert!(state.is_ready_window(42, 1));
        assert_eq!(state.show(), ShowAction::Deferred);
        state.destroyed(42);
        assert!(!state.thread_finished());
        assert_eq!(state.show(), ShowAction::Deferred);
        state.end_removal();
        assert_eq!(state.show(), ShowAction::Start);
    }

    #[test]
    fn show_during_teardown_restarts_only_after_the_thread_finishes() {
        let mut state = WindowLifecycle::new();
        assert_eq!(state.show(), ShowAction::Start);
        state.created(42);
        assert_eq!(state.ready(42), Some((1, true)));
        state.destroyed(42);
        assert_eq!(state.show(), ShowAction::Deferred);
        assert!(state.thread_finished());
        state.created(42);
        assert_eq!(state.ready(42), Some((2, true)));
        assert!(!state.is_ready_window(42, 1));
    }

    #[test]
    fn failed_webview_creation_is_not_published_as_ready() {
        let mut state = WindowLifecycle::new();
        assert_eq!(state.show(), ShowAction::Start);
        state.created(42);
        state.creation_failed();
        state.destroyed(42);
        assert!(!state.thread_finished());
        assert_eq!(state.ready_window(), None);
        assert_eq!(state.show(), ShowAction::Start);
    }

    #[test]
    fn acknowledged_document_cannot_be_replaced_by_navigation() {
        let mut state = WindowLifecycle::new();
        assert_eq!(state.show(), ShowAction::Start);
        let window = state.created(42);
        assert!(!state.admit_initial_document(
            42,
            window,
            "http://local/page/a",
            "https://example.invalid"
        ));
        assert!(state.admit_initial_document(
            42,
            window,
            "http://local/page/a",
            "http://local/page/a"
        ));
        assert_eq!(state.ready(42), Some((window, true)));
        assert!(state.begin_exit());
        assert_eq!(state.exit_observation(), Ok(Some((42, window, 1))));
        assert!(state.is_ready_document(42, window, 1));
        // A reload or a different page cannot replace the document after ACK.
        assert!(!state.admit_initial_document(
            42,
            window,
            "http://local/page/a",
            "http://local/page/a"
        ));
        state.end_exit();
        state.destroyed(42);
        assert!(!state.thread_finished());
        assert_eq!(state.show(), ShowAction::Start);
        let replacement = state.created(42);
        assert_ne!(window, replacement);
        assert!(!state.admit_initial_document(
            42,
            window,
            "http://local/page/a",
            "http://local/page/a"
        ));
        assert!(state.admit_initial_document(
            42,
            replacement,
            "http://local/page/a",
            "http://local/page/a"
        ));
        assert_eq!(state.ready(42), Some((replacement, true)));
        assert!(!state.is_ready_document(42, window, 1));
    }

    #[test]
    fn exit_waits_for_initial_navigation_admission() {
        let mut state = WindowLifecycle::new();
        assert_eq!(state.show(), ShowAction::Start);
        let window = state.created(42);
        assert_eq!(state.ready(42), Some((window, true)));
        assert!(state.begin_exit());
        assert_eq!(state.exit_observation(), Err(()));
        assert!(state.admit_initial_document(
            42,
            window,
            "http://local/page/a",
            "http://local/page/a"
        ));
        assert_eq!(state.exit_observation(), Ok(Some((42, window, 1))));
    }
}
